# Changelog

## Unreleased

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
