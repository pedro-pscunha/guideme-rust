# Changelog

## Unreleased

### Documentation

- README rewritten for clarity, in plain English, with the section order that every guideme SDK
  shares. Design notes, measurements and contributor details now live only in `docs/design.md`,
  `docs/observability.md`, `docs/contract.md`, `AGENTS.md` and `CONTRIBUTING.md`, which already
  recorded them. The README lists the TypeScript SDK. The crates.io page keeps the 0.2.0 README
  until the next release.

## 0.2.0 (2026-09-22)

### Breaking

- `choose_among` takes `(K: Into<String>, Option<R: IntoRubric>)` where it took
  `(&str, Option<&str>)`, and `score_levels` takes `R: IntoRubric` where it took `&str`. A call
  passing `Some("text")`, owned `String`s, a `Cow`, or pairs from a caller's own helper generic
  over `Into<String>` still compiles; `guideme/tests/rubric.rs` carries one call of every shape,
  so the claim is a build failure rather than a sentence. The one shape that needs a change is a
  list where **every** option is bare: name the type once, `[("a", None::<&str>), ("b", None)]`,
  because one list has one rubric type and nothing else says what it is.
- `Error::Overloaded` is `Overloaded { retry_after: Option<Duration> }` where it was a unit
  variant. A `529` parsed the header and then dropped it while a `429` kept it; now both carry
  it.

### Added

- Runtime options and levels carry examples. `choose_among` and `score_levels` accept a
  description or a `Rubric`, so a choice built from a database row gets the same rubric a
  derived enum gets. Rendering is unchanged, byte for byte: `choose::<C>` wraps the derive's
  already-rendered string in a `Rubric` that carries no parts, and a rubric with no parts
  renders to itself.
- The rules that need more than one rubric in view now hold on every path: an example shared
  by two options or two levels, and a counterexample on a level, are `Error::Config` when the
  question is asked, as they are compile errors under the derives. A declaration is legal under
  both or under neither. `docs/contract.md` §3 loses the paragraph that said otherwise.
- `Guide::ask_with_receipt` returns `Receipt<T> { answer, model, usage }`: the versioned model
  that answered and the tokens the call cost, both of which used to reach the span and stop
  there. `ask` is the same call with everything but the answer dropped. `Receipt`, `ModelInfo`
  and `Usage` are re-exported at the crate root.
- Transport injection. `api::ClientBuilder::http(reqwest::Client)` takes a configured client —
  a proxy, a client certificate, a shared pool — and `GuideBuilder::client(api::Client)` takes
  the whole client. Every setting an injected client already carries (`api_key`, `base_url`,
  `max_retries`, `backoff`, `timeout`) is refused **by name** when it is also set on the guide
  builder, and a `timeout` beside `http(..)` likewise: a setting that silently does nothing is
  what this guards against. `from_env()` sets two of them, which the refusal says out loud.
- `api::reqwest` re-exports the `reqwest` guideme links, so the client `http(..)` takes is
  `guideme::api::reqwest::Client` and a caller adds no dependency and tracks no version. Two
  `reqwest` majors in one tree are two unrelated `Client` types, and the error when they
  disagree names neither the cause nor the fix. **The consequence is that a `reqwest` major
  bump is now a breaking change for guideme**; it is recorded under Releasing in `AGENTS.md`.
- `Receipt` is `#[non_exhaustive]`. The response is the API's to grow and what it grows
  belongs here, so adding a field later stays non-breaking for anyone reading the fields.
- `GuideBuilder::from_env()` reads `TYPESAFE_API_KEY` (required), `TYPESAFE_BASE_URL` and
  `GUIDEME_MODEL` (optional) onto a builder you are still configuring, so
  `Guide::builder().from_env()?.policy(HOUSE).build()?` works. `Guide::from_env()` stays as the
  one-liner and delegates to it.
- `GuideBuilder::backoff(Duration)`, which `api::ClientBuilder` had and the guide did not.
- README gains a "Testing your code" section: a complete `#[tokio::test]` answering a noul
  against a `wiremock` server, with no network and no API key.

### Changed

- `GET /v1/models` is retried on `429` and `529`, with the same budget, backoff and spans as
  `POST /v1/systemone`. It was not retried at all, so a throttle on a startup `models()` call
  failed the boot — which the API docs say the SDKs handle. Both endpoints now drive one retry
  loop.
- A failure to connect is retried inside the same budget: a refused or reset connection, a TLS
  handshake failure. The request never reached a server, so sending it again is safe. A
  **timeout of any phase** and a body failure are still not retried. `timeout` is one deadline
  over the whole attempt, so a connect-phase timeout is indistinguishable from a read timeout —
  `reqwest` reports both as `is_timeout()`, not `is_connect()` — and retrying either would
  multiply the wall time that setting promises. With a client handed in through `http(..)` the
  classification is `reqwest`'s over that client, so a `connect_timeout` set there makes connect
  timeouts retryable; guideme never sets one.
- The `guideme.retry` event carries `error.type = "transport"` and **no**
  `http.response.status_code` when the attempt failed before a response. Exactly one of the two
  is on every retry event. This is a telemetry contract change; `docs/observability.md` has the
  field table.
- A compile-time block in `guideme/tests/batch.rs` pins that `Guide::ask` futures are `Send`,
  so `tokio::spawn` keeps working: an `Rc` reaching the future would otherwise break every
  spawning caller on a patch upgrade with nothing to notice.
- `spec/schema/` and `spec/vectors/` are byte-identical to 0.1.1. Neither the wire nor the
  rendering moved.

## 0.1.1 (2026-09-22)

- `#[guide(example = "…")]` and `#[guide(counterexample = "…")]`, both repeatable, compose a
  variant's rubric into the text the model reads: an `Examples:` line and a `Not this option:`
  line, items joined with `; `. `counterexample` is a `Choice` key only. The derive renders the
  string at expansion time, so `Options::RUBRIC`, `Levels::LEVELS`, the wire and the schemas are
  unchanged. A rubric with no examples and no counterexamples renders to itself, byte for byte,
  so every 0.1.0 declaration puts the same bytes on the wire.
- `Rubric`, a small builder for the rubric positions that are not an enum:
  `Rubric::new(what).example(..).counterexample(..)` composes the same way. All three question
  kinds now take examples; a noul's `true`/`false` criteria measured the largest swing of the
  three. `noul(..).criteria(yes, no)` now takes `impl IntoRubric` rather than
  `impl Into<String>`. `IntoRubric` is sealed and covers everything `Into<String>` covers, plus
  `Rubric`, so this is a widening: every 0.1.0 call still compiles, including one made from a
  caller's own helper generic over `Into<String>`. It buys an error channel — an empty example
  or counterexample, a duplicate within a clause, a string that is both an example and a
  counterexample, and examples attached to a blank description are each rejected with
  `Error::Config` when the question is asked, in the derives' wording, as is an example shared
  by a noul's `true` and `false` rubrics. `Rubric::render` is fallible and is the only way to
  turn one into a string, so there is no unchecked path to the wire; `Rubric` is deliberately
  not `Into<String>`, which is what lets `IntoRubric` accept both it and a plain description.
- Declaration-time checks, each a compile error naming the rule: an empty or whitespace-only
  example or counterexample; a duplicate within one variant's examples; `counterexample` on a
  `Levels` derive; the same string as an example of two different variants; and the same string
  as both an example and a counterexample of one variant. The same string as an example of one
  option and a counterexample of another stays legal — that is the confusable-options pattern
  the feature exists for.
- **This release breaks no existing build.** Every new compile error needs a `#[guide(example)]`
  or `#[guide(counterexample)]` to fire, and nothing in 0.1.0 could have written one. In
  particular a variant whose rubric is empty or whitespace — `#[guide(rubric = "")]`, or a bare
  `///` — still compiles exactly as it did: it is rejected only when examples were attached to
  it, which is the "you described nothing" case. The Python SDK draws the line in the same
  place, so the same declaration is legal or illegal in both.
- Examples and counterexamples render in declaration order, and the `Not this option` label is
  fixed. Both are contract, and `spec/vectors/rubric.json` covers them.
- An example or counterexample may not contain `U+000A` or `U+000D`: items are joined onto one
  line, so a line break in one would render as a clause the declaration never wrote. A rubric's
  description may still contain anything. Items are new in this release, so nothing can already
  depend on it — which is why the rule ships with the feature rather than after it.
- `spec/vectors/rubric.json`: the rendering is a cross-SDK contract item, published as golden
  cases generated from real derived enums. `spec/schema/` and `spec/vectors/policy.json` are
  byte-identical to 0.1.0.

## 0.1.0 (2026-09-21)

First release.

- `Guide::ask(shape, state)`: one verb over any question shape (single, tuple of up to 8,
  `Vec`, `BTreeMap`), one request and one `guideme.ask` span per call.
- Questions as values: `noul`, `choose::<C>`, `score::<L>`, `choose_among`, `score_levels`,
  with `.with(Policy)`, `.yes_above`, `.no_below`, `.min_confidence`, `.or`, `.detail`, and
  `.criteria` on noul.
- `#[derive(Choice)]` and `#[derive(Levels)]`: doc comments become rubrics, keys default to
  snake_case, `#[guide(key|rubric|fallback)]`, compile-time rule checks.
- Pure `policy::resolve` with `Policy` patches and validated `Thresholds`; the unsure ladder
  `.or` > enum fallback > `Error::Unsure`.
- `api` wire mirror with `Client`: retries on `429`/`529` honouring `retry-after`, typed
  errors, `GET /v1/models`.
- `spec/`: JSON Schemas and golden policy vectors as the contract every guideme SDK satisfies.
- Telemetry shaped by the OpenTelemetry semantic conventions: the `guideme.ask` span with
  `gen_ai.*`, `server.*` and `guideme.*` attributes, one HTTP client span per attempt with
  `http.*` and `url.*` attributes, a `guideme.retry` warning when throttled, one
  `guideme.answer` event per question, and `error.type` from `Error::kind()` on failure.
  Field names are part of the cross-SDK contract. `examples/otlp` exports traces and logs
  linked by trace id, configured through the standard `OTEL_*` variables.
