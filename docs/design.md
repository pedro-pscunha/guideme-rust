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
  their types, and `question.rs`, `api/` and the wire are untouched.
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
- **A rubric with no examples renders to itself.** This is what keeps 0.1.1 non-breaking, and
  it is why `what` is never trimmed or re-punctuated. `spec/vectors/rubric.json` pins it, and
  `guideme-derive` property-tests it.
- **`extern crate self as guideme`.** `spec.rs` declares the rubric vector's enums through this
  crate's own derives, which expand to `::guideme` paths. The alias is what makes those paths
  resolve inside the crate, and it is the price of generating the vector from real derived
  enums instead of a second copy of the renderer.
- **Fallback use is not on the answer event.** The event is emitted from the resolved outcome, before typed decoding chooses `.or`/enum fallback. The settled thresholds are on the event, so "why unsure" is answerable.
