# Observability

`guideme` emits `tracing` spans and events only. Bridge them to OpenTelemetry with
`tracing-opentelemetry`; the crate ships no metrics exporter.

## Span `guideme.ask` (one per request)

| Field | Type | Meaning |
|---|---|---|
| `model.requested` | str | alias or id sent (`jev-latest` by default) |
| `model.answered` | str | versioned id that answered (`jev-1.13.0`) |
| `questions` | u64 | questions in the request |
| `state.bytes` | u64 | JSON length of the state |
| `state` | str | the state JSON; only with `GuideBuilder::record_state(true)` |
| `usage.input_tokens` | u64 | billed tokens |
| `usage.output_tokens` | u64 | free tokens |
| `retries` | u64 | `429`/`529` retries performed |
| `elapsed_ms` | u64 | wall time including retries |
| `error` | str | the `Error` display, only when the ask failed |

State is user data and is never recorded unless opted in. The API key never appears in any
span, event, or `Debug` output.

## Event `guideme.answer` (one per question, inside the span)

| Field | Type | Meaning |
|---|---|---|
| `question` | str | `q0..qN`, encounter order |
| `kind` | str | `noul` / `choice` / `score` |
| `outcome` | str or u64 | `yes`/`no`/`unsure`; the chosen key; the argmax level index |
| `probability` | f64 | noul only: the probability of yes |
| `confidence` | f64 | choice and score: the reported confidence |
| `value` | f64 | score only: the expected value |
| `unsure` | bool | the policy's verdict |
| `yes_above`, `no_below`, `min_confidence` | f64 | the settled thresholds that produced the verdict |

Whether an unsure answer was then resolved by `.or(..)` or an enum fallback is not on the
event in 0.1.0; it is decided after the event, at typed decoding.

## Errors

Every failure is a typed `guideme::Error`. `Error::Unsure { question, value, threshold }`
names the question id, the judged number, and the boundary it fell short of.
