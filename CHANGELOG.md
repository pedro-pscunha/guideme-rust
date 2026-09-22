# Changelog

## 0.1.1 (2026-09-21)

- `#[guide(example = "…")]` and `#[guide(counterexample = "…")]`, both repeatable, compose a
  variant's rubric into the text the model reads: an `Examples:` line and a `Not this option:`
  line, items joined with `; `. `counterexample` is a `Choice` key only. The derive renders the
  string at expansion time, so `Options::RUBRIC`, `Levels::LEVELS`, the wire and the schemas are
  unchanged. A rubric with no examples and no counterexamples renders to itself, byte for byte,
  so every 0.1.0 declaration puts the same bytes on the wire.
- `Rubric`, a small builder for the rubric positions that are not an enum:
  `Rubric::new(what).example(..).counterexample(..)` composes the same way and is
  `Into<String>`, so `noul(..).criteria(yes, no)` takes it with no signature change and a plain
  pair of strings keeps working. All three question kinds now take examples; a noul's
  `true`/`false` criteria measured the largest swing of the three.
- Declaration-time checks, each a compile error naming the rule: an empty or whitespace-only
  rubric, example or counterexample; a duplicate within one variant's examples; an example on a
  variant with no rubric; `counterexample` on a `Levels` derive; the same string as an example
  of two different variants; and the same string as both an example and a counterexample of one
  variant. The same string as an example of one option and a counterexample of another stays
  legal — that is the confusable-options pattern the feature exists for.
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
