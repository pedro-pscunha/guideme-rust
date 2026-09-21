# Changelog

## Unreleased

- Telemetry follows the OpenTelemetry semantic conventions. The `guideme.ask` span carries
  `gen_ai.provider.name`, `gen_ai.operation.name`, `gen_ai.request.model`,
  `gen_ai.response.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`,
  `server.address`, `server.port`, and is exported with kind `client`. Its own fields are
  namespaced: `guideme.questions`, `guideme.state.bytes`, `guideme.state`. The previous
  `model.*`, `usage.*`, `questions`, `state*`, `retries` and `elapsed_ms` names are gone.
- Every HTTP attempt is a child span named `POST /v1/systemone` (or `GET /v1/models`) with
  `http.request.method`, `url.full`, `http.response.status_code` and, on retries,
  `http.request.resend_count`. A throttled attempt emits a `guideme.retry` event at `WARN`.
- Failures set `error.type` to the new `Error::kind()` name and the OpenTelemetry status to
  error with the message as description; the `error` field is gone. Response bodies stay
  off the span.
- `guideme.answer` event fields are namespaced `guideme.*` and the event carries a message.
- Spans and events use the targets `guideme` and `guideme::api` instead of module paths.
- `ClientBuilder::build` rejects a base URL without a host or port, or with credentials in
  it, because the URL is recorded on every request span.
- The telemetry names are part of the cross-SDK contract; `docs/contract.md` says so. No
  other SDK exists yet, so there is nothing to announce to.
- `examples/otlp` exports traces and logs, linked by trace and span id, configured through
  the standard `OTEL_*` variables, with a collector config that prints what it receives.

## 0.1.0 (2026-09-20)

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
