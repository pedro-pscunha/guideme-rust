# Design

`guideme` is four modules behind one verb. Vocabulary used below: a **module** has an
**interface** and an **implementation**; a **seam** is where the interface lives; a module is
**deep** when a lot of behaviour sits behind a small interface.

## Seams

| Module | Interface | What it hides |
|---|---|---|
| `Guide` (`guide.rs`) | `ask(shape, state)`, `models()`, `with_policy`, builder | request assembly, question-id minting, one span per request, per-answer events, policy precedence, decode of every shape |
| `policy` (`policy.rs`) | `resolve(&Answer, Thresholds) -> Outcome`, `Policy`/`Thresholds` | threshold arithmetic for all three primitives, validation, ranking. Pure: no I/O, no generics. This is the function the shared contract pins. |
| `api::Client` (`api/client.rs`) | `evaluate`, `models` | HTTP, auth header, retry with backoff and `retry-after`, status → `Error` mapping, body decode |
| `Ask` shapes (`ask.rs`) | sealed trait over `Question<K>`, tuples, `Vec`, `BTreeMap` | id assignment in encounter order, per-question settled thresholds, decoding back into the same shape |
| `Kind`s (`question.rs`) | `noul`, `choose`, `score`, `choose_among`, `score_levels`; `.with/.or/.detail/.criteria` and the three threshold one-liners | wire encoding per primitive, rubric membership checks, `Outcome` → typed value, the unsure ladder |
| derives (`guideme-derive`) | `#[derive(Choice)]`, `#[derive(Levels)]` | doc comment → rubric, `example`/`counterexample` → the rendered rubric, snake_case keys, fallback, compile-time rule checks |
| `Rubric` (`rubric.rs`) | `Rubric::new(what).example(..).counterexample(..)`, `Into<String>` | the same composition for the rubric positions that are not an enum |

## The deletion test

- Delete `policy` and threshold logic reappears in every caller, three times (one per primitive), and the shared contract disappears.
- Delete `ask` and batching reappears as one method per shape (`batch`, `batch_all`, `batch_map`, …) with id bookkeeping in each.
- Delete `api::client` and retry/backoff reappears wherever the API is called.
- Delete `Guide` and span assembly, id minting, and the precedence merge reappear at every call site.
- Delete the derives and every enum carries a hand-written rubric table that can drift from its variants.

Each module survives the test.

## Decisions

- **Questions are values.** `noul(..)`, `choose::<C>(..)`, `score::<L>(..)` need no `Guide`; build one once, store it, clone it per ask, send it across threads. The `Guide` is only an executor and consumes the question it is given.
- **One verb.** `guide.ask(shape, state)`. Question first, state second, so `if guide.ask(noul("…"), &t)` reads with the judgment early.
- **Batching is the shape.** A tuple of questions is a question. So is a `Vec` or a `BTreeMap`. Output has the same shape. One request, one span, ids `q0..qN` in encounter order.
- **Policy is a patch; thresholds are settled.** `Policy` has `Option` fields and `const fn` setters, so house policies are constants. Precedence: question > guide > defaults `0.5 / 0.5 / 0.0`. `Thresholds` is validated on construction and is what `resolve` and the golden vectors take.
- **The unsure ladder.** `.or(value)` beats `#[guide(fallback)]` beats `Error::Unsure`. `.detail()` switches to `Verdict` / `Ranked<C>` / `Scored<L>` and never fails on unsure.
- **Score plain output is the argmax level.** `.detail()` exposes the API's expected `value` too.
- **`Levels` does not generate `Ord`.** Callers derive `PartialOrd, Ord`; declaration order equals level order, so the two cannot disagree.
- **No `rand`.** Backoff jitter comes from `std::hash::RandomState`.
- **Rubric examples are flattened into the rubric string, not sent as structured criteria.**
  The API natively supports them: `criteria` takes an object per option, and the TypeSafe docs
  show the `what` / `not_for` / `examples` shape we would want. We do not use it. A score answer
  echoes the criteria back in `legend`, which is `BTreeMap<u8, String>` here and
  `Vec<String>` on `policy::Outcome::Score`, so object criteria fail to deserialise — sending
  them means changing a public type, in every SDK, for a field none of them reads. Measured
  against live Jev on 2026-09-21, flattening is also as good (score 1.01/1.01/1.01 at
  confidence 0.99 against 1.04/1.02/1.04 at 0.94 for structured, on the docs' own worked
  example) and cheaper (400 against 450 billed input tokens for the same content). The gain
  comes from the examples being present, not from the JSON structure. So the composition
  happens in the proc macro at expansion time, `Options::RUBRIC` and `Levels::LEVELS` keep
  their types, and `question.rs`, `api/` and the wire are untouched. One of the three reasons
  lapsed at 0.2.0 — the runtime constructors now take `Rubric`, so "it would change a public
  type" no longer applies to them — and the two measured ones, equal quality and 400 against
  450 billed tokens, are what keeps it rejected.
- **No synchronous `Guide`.** A blocking twin would be a second execution mode over the same
  policy layer: either a duplicated `Client` on `reqwest::blocking`, which cannot be called
  from inside a runtime, or a `block_on` wrapper, which deadlocks on a current-thread runtime
  and is a footgun in exactly the applications that would reach for it. Rust callers who need
  one already have `Runtime::block_on` at their own boundary, where they can see which runtime
  they are in. Python has two guides because its ecosystem is genuinely split; Rust's is not.
- **All three kinds take examples, and a noul gets them through a builder.** A noul's
  `true`/`false` criteria are as confusable as a choice's options, and measured the largest
  swing of the three: on `Our nightly export job has been failing since Tuesday. We pull the
  numbers by hand for now.` asked as "Is this ticket urgent?", plain criteria answer 0.75 and
  criteria carrying examples answer 0.25 — a 0.49 swing, to the correct answer, because one of
  the `false` examples is "a broken job with a manual workaround". A noul has no enum to hang
  attributes off, so the parts arrive as `Rubric`. It is a builder rather than a second
  `criteria` method: `criteria` takes `impl IntoRubric`, a sealed trait with a blanket case for
  everything `Into<String>` covers and one more for `Rubric` itself. That is a true widening —
  every call that compiled against `impl Into<String>` still compiles, including a caller's own
  helper generic over `Into<String>`, which no set of `From<T> for Rubric` conversions can
  reach. The price is that `Rubric` must not be `Into<String>`, or the two cases overlap, so it
  renders through `Rubric::render` instead — and permanently, since adding the conversion later
  would make the two `IntoRubric` impls overlap. Widening rather than overloading is what gives
  the runtime path an error channel.
- **Compatibility claims get compiled, not reasoned about.** The first attempt widened to
  `impl Into<Rubric>` with `From<&str>` and `From<String>`, and argued from those that nothing
  broke. Compiling one call of every shape `String` converts from found four that did —
  `&String`, `Cow<'_, str>`, `Box<str>`, `char` — and compiling a caller's generic helper found
  a fifth that conversions could not fix at all. Every one was invisible to inspection.
  `guideme/tests/rubric.rs` carries a `const _: fn() = || { … }` block of one call per shape,
  so the claim is a build failure rather than a sentence.
- **`Not this option` is the measured label.** Four runs each on the confusable-options case,
  all three candidates correct 4/4: `Not this option` 0.865 mean probability on the right
  option, `Not` 0.845, `Counterexamples` 0.830.
- **The renderer is duplicated once, on purpose.** `Rubric` renders at runtime and the proc
  macro renders at expansion time, because `Options::RUBRIC` is a `const` and cannot call a
  function. That is ~10 lines in two places. The alternatives are worse: a third published
  crate to share them, forever, or turning `RUBRIC` into a function, which breaks a public
  trait and forces 0.2.0. `guideme/tests/rubric.rs` pins the two together by asserting a real
  derived enum's `RUBRIC` equals `Rubric`'s output for the same parts, on every golden row.
- **Newline separation, no terminal punctuation.** Examples frequently end in `?`, and a
  space-joined format then needs a trailing `.` that produces `Where is my refund?.`. A
  newline needs no punctuation heuristic and measured equivalent in quality, for two extra
  tokens. `what` is used verbatim, which is what makes a rubric with no examples render to
  itself byte for byte.
- **Telemetry speaks OpenTelemetry.** The ask span uses the GenAI conventions, each HTTP attempt is its own client span with the HTTP conventions, and a failure is `error.type` plus an error status on the span rather than an `ERROR` event. Anything without a convention is namespaced `guideme.`. The crate depends on `tracing` only; `docs/observability.md` shows the exporter side.

## Sharp edges

- **A batch is atomic.** One answer that resolves to `Error::Unsure` (or `Protocol`) fails the whole call. Use `.or(..)`, an enum fallback, or `.detail()` on the questions that may be unsure.
- **Ids are visible.** `q{n}` appears on the wire, in `Error::Unsure`, and in `guideme.answer` events. They are positions in encounter order, nothing more.
- **`Key` and `Rank`** are only meaningful through `choose_among` and `score_levels`. `choose::<Key>(..)` type-checks but is rejected at ask time with `Error::Config` because its rubric is empty.
- **Two `Question`s.** `guideme::Question<K>` is the user-facing value; `guideme::api::Question` is the wire enum it becomes.
- **`State` takes references.** `guide.ask(q, &ticket)` for any `Serialize` type; an owned struct is not accepted (coherence with the `String` and `Value` impls). Text literals work directly.
- **Declaration order is contract.** Examples and counterexamples render in the order written.
  Sorting them would be invisible locally and would silently diverge from another SDK, so
  `spec/vectors/rubric.json` carries a case whose clauses are deliberately not in sorted order.
- **An example cannot belong to two options.** Saying so asserts the input is both, which
  cannot be true, and it is a compile error — as is the same string as an example and a
  counterexample of the same option. The same string as an example of one option and a
  counterexample of another stays legal: that is the confusable-options pattern the feature
  exists for, and `guideme/tests/live.rs` uses it.
- **`Rubric` is checked when the question is asked, not when it is built.** A builder has no
  `Result` to return, so the checks live in `Rubric::render`, which is fallible and is the only
  way to turn a rubric into a string. There is no `Display`: an infallible renderer would be the
  path every caller reaches for and every check would be optional. Everything one rubric can see
  is checked there — an empty entry, a duplicate within a clause, a string that is both an
  example and a counterexample, examples attached to a blank rubric — in the derive's wording,
  so the two read as one rule. The rules that need more than one rubric in view are checked
  where the whole set is held at once, which since 0.2.0 is every runtime path: a noul's pair,
  `choose_among`'s options and `score_levels`'s levels. A declaration is legal under the derive
  and at runtime, or under neither.
- **A rubric with no examples renders to itself.** This is what keeps 0.1.1 non-breaking, and
  it is why `what` is never trimmed or re-punctuated. `spec/vectors/rubric.json` pins it, and
  `guideme-derive` property-tests it.
- **`extern crate self as guideme`.** `spec.rs` declares the rubric vector's enums through this
  crate's own derives, which expand to `::guideme` paths. The alias is what makes those paths
  resolve inside the crate, and it is the price of generating the vector from real derived
  enums instead of a second copy of the renderer.
- **Fallback use is not on the answer event.** The event is emitted from the resolved outcome, before typed decoding chooses `.or`/enum fallback. The settled thresholds are on the event, so "why unsure" is answerable.
