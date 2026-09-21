# Observability

guideme emits `tracing` spans and events and nothing else. It installs no subscriber, writes
to no file, and starts no exporter. The application decides where the data goes.

## What is emitted

One span per call to `Guide::ask`, with one event per question inside it. Both are at level
`INFO` and carry the target `guideme::guide`.

### Span `guideme.ask`

| Field | Type | Meaning |
|---|---|---|
| `model.requested` | str | alias or id sent, `jev-latest` by default |
| `model.answered` | str | versioned id that answered, for example `jev-1.13.0` |
| `questions` | i64 | questions in the request |
| `state.bytes` | i64 | JSON length of the state |
| `state` | str | the state JSON, only when `record_state(true)` is set |
| `usage.input_tokens` | i64 | billed tokens |
| `usage.output_tokens` | i64 | free tokens |
| `retries` | i64 | 429 and 529 retries performed |
| `elapsed_ms` | i64 | wall time including retries |
| `error` | str | the `Error` display, only when the ask failed |
| `otel.status_code` | str | `ERROR`, only when the ask failed |

`model.answered`, `usage.*` and `retries` are recorded when the response arrives, so they are
absent on a span that failed in transport. `state` is user data and is never recorded unless
asked for. The API key appears in no field.

`otel.status_code` exists for one reason: `tracing-opentelemetry` reads that field name and
sets the exported span's status from it, which is what dashboards filter on. Subscribers that
do not know the name treat it as an ordinary field.

### Event `guideme.answer`

| Field | Type | Meaning |
|---|---|---|
| `question` | str | `q0..qN`, encounter order |
| `kind` | str | `noul`, `choice` or `score` |
| `outcome` | str or i64 | `yes`/`no`/`unsure`, the chosen key, or the level index |
| `probability` | f64 | noul only, the probability of yes |
| `confidence` | f64 | choice and score, the reported confidence |
| `value` | f64 | score only, the expected value |
| `unsure` | bool | the policy's verdict |
| `yes_above`, `no_below`, `min_confidence` | f64 | the settled thresholds behind the verdict |

The event is emitted from the resolved outcome, before the unsure ladder picks a fallback, so
it says what the model answered rather than what the caller ended up with.

Numbers are recorded as `i64` rather than `u64` on purpose. OpenTelemetry attributes are
signed and `tracing-opentelemetry` has no unsigned path, so a `u64` field would export as a
string and stop being aggregatable.

## Levels and filtering

Everything guideme emits is `INFO`. There are no `DEBUG` or `TRACE` events, and the crate
never emits an `ERROR` event: a failure is returned as a typed `Error` and marked on the span.
Filtering guideme above `INFO` therefore yields nothing at all.

| `RUST_LOG` | Effect |
|---|---|
| `guideme=info` | spans and answer events |
| `warn,guideme=info` | the same, with the rest of the application quiet |
| `guideme=off` | guideme silent |
| `guideme::guide=info` | same as `guideme=info`; everything is emitted from that module |
| `info` | guideme plus every other crate at info, which includes hyper and tonic |

## Console

```rust
use tracing_subscriber::{EnvFilter, fmt};

fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,guideme=info")))
    .init();
```

One line of output per answer, with the span's fields as context:

```
INFO guideme.ask{model.requested="jev-latest" questions=3 state.bytes=122 elapsed_ms=332
  model.answered="jev-1.13.0" usage.input_tokens=422 usage.output_tokens=71 retries=0}:
  guideme::guide: question="q0" kind="choice" outcome="billing" confidence=0.99
  unsure=false yes_above=0.5 no_below=0.5 min_confidence=0.6
```

## OTLP

`examples/otlp` is a runnable version of everything below.

The four OpenTelemetry crates must agree on a version of `opentelemetry` or the tracer will
not satisfy the layer's trait bound. The numbering is offset: `tracing-opentelemetry` 0.33
builds against `opentelemetry` 0.32.

```toml
tracing-opentelemetry = "0.33"
opentelemetry = "0.32"
opentelemetry_sdk = { version = "0.32", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.32", features = ["grpc-tonic"] }
```

```rust
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

let exporter = opentelemetry_otlp::SpanExporter::builder().with_tonic().build()?;

let provider = SdkTracerProvider::builder()
    .with_batch_exporter(exporter)
    .with_resource(Resource::builder().with_service_name("support-triage").build())
    .build();

let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,guideme=info"));

tracing_subscriber::registry()
    .with(fmt::layer().with_filter(filter()))
    .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("guideme")).with_filter(filter()))
    .init();

// ... the application runs ...

provider.shutdown()?; // the batch exporter drops what it is holding without this
```

The exporter reads `OTEL_EXPORTER_OTLP_ENDPOINT` and defaults to `http://localhost:4317`.
Use `.with_http()` and port 4318 for OTLP over HTTP. Each layer carries its own filter, so
the console and the exporter can be tuned separately.

A span reaches the collector when it closes, which is when `ask` returns, so the exported
span already carries the fields recorded from the response.

### What arrives

Answer events become span events. Recorded against
`otel/opentelemetry-collector-contrib` with the debug exporter:

```
Span #1
    Name           : guideme.ask
    Status code    : Unset
Attributes:
     -> code.module.name: Str(guideme::guide)
     -> model.requested: Str(jev-latest)
     -> questions: Int(3)
     -> state.bytes: Int(122)
     -> elapsed_ms: Int(343)
     -> model.answered: Str(jev-1.13.0)
     -> usage.input_tokens: Int(422)
     -> usage.output_tokens: Int(71)
     -> retries: Int(0)
Events:
SpanEvent #1
     -> Name: guideme.answer
     -> Attributes::
          -> question: Str(q1)
          -> kind: Str(score)
          -> outcome: Int(1)
          -> value: Double(1.43)
          -> confidence: Double(0.39)
          -> unsure: Bool(false)
          -> min_confidence: Double(0)
```

A failed ask arrives with the status set:

```
    Name           : guideme.ask
    Status code    : Error
     -> error: Str(unsure answer for question q0: 0.36 against threshold 0.999)
```

## Useful queries

- Cost per call: sum `usage.input_tokens` by `model.answered`.
- Provider trouble: count spans where `retries` is above zero, or where `error` matches a
  rate limit or overload.
- Threshold tuning: on `guideme.answer`, the rate of `unsure=true` per `question`, compared
  against the `min_confidence` recorded beside it. A band that is too wide shows up as
  reviewers drowning; too narrow shows up as wrong routes.
- Drift: `error` containing `protocol violation` means the API's shape changed.
