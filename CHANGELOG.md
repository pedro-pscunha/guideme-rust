# Changelog

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
