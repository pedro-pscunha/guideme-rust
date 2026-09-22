# Observability

guideme emits `tracing` spans and events and nothing else. It installs no subscriber, writes
to no file, and starts no exporter. The application decides where the data goes.

What it emits is shaped by the OpenTelemetry semantic conventions: the GenAI conventions
for the ask span, the HTTP client conventions for each request, and the error conventions
for failures. Any OTLP backend, and any product that understands `gen_ai.*` attributes,
reads it without a mapping step.

## Shape

```
guideme.ask                      span, kind client, one per Guide::ask
├── POST /v1/systemone           span, kind client, one per HTTP attempt
│   └── guideme.retry            WARN event, only when that attempt is about to be retried
└── guideme.answer               INFO event, one per question
```

The ask span and the answer events carry the target `guideme`; the HTTP spans and the retry
event carry `guideme::api`. A batch of three questions is one ask span, one HTTP span if the
first attempt succeeds, and three answer events. A retried request is one ask span with
sibling HTTP spans, each with its own status code. `Guide::models` has no ask span of its
own: it emits a bare `GET /v1/models` span under whatever span the caller is in, one per
attempt, because that endpoint is retried on the same statuses as `POST /v1/systemone`.

### Span `guideme.ask`

| Field | Type | Meaning |
|---|---|---|
| `otel.kind` | str | `client` |
| `gen_ai.provider.name` | str | `typesafe` |
| `gen_ai.operation.name` | str | `ask` |
| `gen_ai.request.model` | str | alias or id sent, `jev-latest` by default |
| `gen_ai.response.model` | str | versioned id that answered, for example `jev-1.13.0` |
| `gen_ai.usage.input_tokens` | i64 | billed tokens |
| `gen_ai.usage.output_tokens` | i64 | free tokens |
| `server.address`, `server.port` | str, i64 | where the request went |
| `guideme.questions` | i64 | questions in the request |
| `guideme.state.bytes` | i64 | JSON length of the state |
| `guideme.state` | str | the state JSON, only when `record_state(true)` is set |
| `error.type` | str | [`Error::kind`], only when the ask failed |
| `otel.status_code` | str | `ERROR`, only when the ask failed |
| `otel.status_description` | str | the error message, only when the ask failed |

`gen_ai.response.model` and `gen_ai.usage.*` are recorded when the response arrives, so they
are absent on a span that failed in transport. `guideme.state` is user data and is never
recorded unless asked for. The API key appears in no field.

Two deliberate deviations from the GenAI conventions. The span keeps the name `guideme.ask`
rather than the `{operation} {model}` pattern: a fixed name is what `RUST_LOG` and
dashboards key on, and the model is on the span as an attribute. And
`gen_ai.operation.name` is the custom value `ask` rather than a well-known one such as
`chat`: a Jev judgment sends structured questions and gets probabilities back, and calling
it a chat would make products that key on that value read it as one.

### Spans `POST /v1/systemone` and `GET /v1/models`

| Field | Type | Meaning |
|---|---|---|
| `otel.kind` | str | `client` |
| `http.request.method` | str | `POST` or `GET` |
| `server.address`, `server.port` | str, i64 | host and port of the base URL |
| `url.full` | str | the request URL; a base URL carrying credentials is rejected at build time so this never holds a secret |
| `url.template` | str | `/v1/systemone` or `/v1/models`, the low-cardinality form of the path |
| `http.request.resend_count` | i64 | ordinal of the retry, absent on the first attempt |
| `http.response.status_code` | i64 | absent when no response arrived |
| `error.type` | str | the status code as text when a response arrived, else [`Error::kind`] |
| `otel.status_code` | str | `ERROR` on any non-200 status or transport failure |

A `429` that was retried and then succeeded is one failed attempt span next to one
successful one, exactly as the HTTP conventions describe a resend.

The HTTP conventions leave the status unset for a 3xx and let an instrumentation with more
context about the request set it more precisely. guideme has that context: its contract
defines `200` as the only success, redirects are followed by the client, and anything else
that reaches this code is returned as an error. So every non-200 marks the attempt.

### Event `guideme.answer`

| Field | Type | Meaning |
|---|---|---|
| `guideme.question` | str | `q0..qN`, encounter order |
| `guideme.kind` | str | `noul`, `choice` or `score` |
| `guideme.outcome` | str or i64 | `yes`/`no`/`unsure`, the chosen key, or the level index |
| `guideme.probability` | f64 | noul only, the probability of yes |
| `guideme.confidence` | f64 | choice and score, the reported confidence |
| `guideme.value` | f64 | score only, the expected value |
| `guideme.unsure` | bool | the policy's verdict |
| `guideme.yes_above`, `guideme.no_below`, `guideme.min_confidence` | f64 | the settled thresholds behind the verdict |

The message is `q0 noul: yes`, `q0 choice: billing` or `q1 score: level 1`. The event is
emitted from the resolved outcome, before the unsure ladder picks a fallback, so it says
what the model answered rather than what the caller ended up with.

### Event `guideme.retry`

Emitted at `WARN` inside the failed attempt's span, just before the wait.

| Field | Type | Meaning |
|---|---|---|
| `http.response.status_code` | i64 | `429` or `529`; absent when no response arrived |
| `error.type` | str | `transport`; present only when no response arrived |
| `guideme.retry.attempt` | i64 | ordinal of the resend about to be made; `1` for the first retry |
| `guideme.retry.delay_ms` | i64 | how long guideme is about to wait |

**Exactly one of `http.response.status_code` and `error.type` is present on every retry
event.** A response that was throttled carries its status; an attempt that never reached a
server — a refused or reset connection, a TLS handshake failure — has no status to report and
carries `error.type = "transport"` instead. Those are retried inside the same budget and with
the same backoff, because the request went nowhere. A timeout of any phase and a body failure
are not retried, so they never produce a retry event: they mark the attempt's span and are
returned. The attempt's own span is marked failed with `error.type = "transport"` either way,
as it already was for a transport failure that was not retried.

The message is `429 from TypeSafe, retrying in 1000 ms`, or `could not reach TypeSafe,
retrying in 500 ms`.

### Errors

guideme never emits an `ERROR` event. A failure is returned as a typed [`Error`] and marked
on the span: `error.type` gets the stable name from [`Error::kind`], and the OpenTelemetry
status becomes `Error` with the message as its description. That is what dashboards filter
on, and it keeps the caller in charge of whether and where the failure is logged.

`error.type` values on the ask span: `auth`, `invalid`, `rate_limited`, `overloaded`,
`transport`, `unexpected_status`, `protocol`, `unsure`, `config`. On an HTTP span it is the
status code as text when a response arrived, otherwise one of those names.

The status description is the error's message, except for `invalid` and
`unexpected_status`: those errors carry the verbatim response body, which could echo the
state, so the span only says which status it was and the body stays on the returned error.

The `otel.*` fields exist because `tracing-opentelemetry` reads those names to set the
exported span's kind and status. Subscribers that do not know them treat them as ordinary
fields.

Numbers are recorded as `i64`, never `u64`. OpenTelemetry attributes are signed and
`tracing-opentelemetry` has no unsigned path, so a `u64` field would export as a string and
stop being aggregatable.

## Levels and filtering

Spans and answer events are `INFO`. The retry event is `WARN`. There is nothing at `DEBUG`
or `TRACE`, and nothing at `ERROR`.

| `RUST_LOG` | Effect |
|---|---|
| `guideme=info` | everything guideme emits |
| `guideme=warn` | retry warnings only; no spans, no answers |
| `guideme=info,guideme::api=warn` | ask spans and answers, retry warnings, no HTTP spans |
| `warn,guideme=info` | everything guideme emits, the rest of the application quiet |
| `guideme=off` | silent |
| `info` | guideme plus every other crate at info, which includes hyper and tonic |

## Console

```rust
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{EnvFilter, fmt};

fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,guideme=info")))
    .with_span_events(FmtSpan::CLOSE) // one line per span close, with its duration
    .init();
```

One line per answer, with the span's fields as context, and one line when each span
closes. Captured from the live API:

```
INFO guideme.ask{otel.kind="client" gen_ai.provider.name="typesafe" gen_ai.operation.name="ask"
  gen_ai.request.model="jev-latest" server.address="api.typesafe.ai" server.port=443
  guideme.questions=3 guideme.state.bytes=122 gen_ai.response.model="jev-1.13.0"
  gen_ai.usage.input_tokens=422 gen_ai.usage.output_tokens=71}:
  guideme: q0 choice: billing guideme.question="q0" guideme.kind="choice" guideme.outcome="billing"
  guideme.confidence=0.99 guideme.unsure=false guideme.yes_above=0.5 guideme.no_below=0.5
  guideme.min_confidence=0.6
INFO guideme.ask{...}: guideme: close time.busy=545µs time.idle=338ms
```

## OTLP

`examples/otlp` is a runnable version of everything below: console, OTLP traces and OTLP
logs, against the live API, with a collector config that prints what it receives.

### Versions

The `opentelemetry` crates must all agree on one version or the tracer will not satisfy the
layer's trait bound, and the numbering is offset: `tracing-opentelemetry` 0.33 builds
against `opentelemetry` 0.32.

```toml
tracing-opentelemetry = "0.33"
opentelemetry = "0.32"
opentelemetry_sdk = { version = "0.32", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.32", features = ["grpc-tonic", "logs", "trace"] }
opentelemetry-appender-tracing = "0.32"
```

### Setup

```rust
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, SpanExporter};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::filter::{FilterExt, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

let resource = Resource::builder().with_service_name("support-triage").build();

let traces = SdkTracerProvider::builder()
    .with_batch_exporter(SpanExporter::builder().with_tonic().build()?)
    .with_resource(resource.clone())
    .build();
let logs = SdkLoggerProvider::builder()
    .with_batch_exporter(LogExporter::builder().with_tonic().build()?)
    .with_resource(resource)
    .build();

let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,guideme=info"));

tracing_subscriber::registry()
    .with(fmt::layer().with_filter(filter()))
    .with(
        tracing_opentelemetry::layer()
            .with_tracer(traces.tracer("my-service"))
            .with_location(false)
            .with_threads(false)
            .with_tracked_inactivity(false)
            .with_filter(filter().and(filter_fn(|meta| meta.is_span()))),
    )
    .with(OpenTelemetryTracingBridge::new(&logs).with_filter(filter()))
    .init();

// ... the application runs ...

traces.shutdown()?; // the batch exporters drop what they are holding without this
logs.shutdown()?;
```

Three layers, three destinations, one filter each:

- `fmt` prints spans and events to the console.
- `tracing_opentelemetry` exports spans. The `filter_fn` keeps events out of it, so each
  event is stored once, as a log record, instead of once more as a span event. Drop the
  `filter_fn` for a backend that has traces but no logs, and the answer events arrive
  attached to the ask span.
- `OpenTelemetryTracingBridge` exports events as OTLP log records. Each record carries the
  trace id and span id of the span it was emitted in, because `tracing_opentelemetry`
  activates the OpenTelemetry context whenever a span is entered. Nothing else is needed for
  the link.

The three layer toggles turn off attributes the exporter would otherwise add to every span:
the source file path, line and module, the thread id and name, and busy and idle
nanoseconds. They are process facts rather than request facts, and the file path names the
build machine. Turn any of them back on when you want it.

A span reaches the collector when it closes, which is when `ask` returns, so the exported
span already carries the fields recorded from the response.

### Configuration by environment

The Rust SDK reads the standard variables, so the code above needs no change to point at a
different backend.

| Variable | Effect |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | where to send; default `http://localhost:4317` for gRPC, `4318` for HTTP |
| `OTEL_EXPORTER_OTLP_HEADERS` | `key=value,key=value`, the usual place for a vendor's API key |
| `OTEL_EXPORTER_OTLP_TIMEOUT` | milliseconds per export batch, default `10000` |
| `OTEL_EXPORTER_OTLP_COMPRESSION` | `gzip` |
| `OTEL_EXPORTER_OTLP_TRACES_*`, `OTEL_EXPORTER_OTLP_LOGS_*` | the same, per signal; these win over the generic ones |
| `OTEL_SERVICE_NAME` | `service.name` on every span and log record |
| `OTEL_RESOURCE_ATTRIBUTES` | `key=value,...`, for example `deployment.environment.name=prod,service.version=1.4.0` |
| `OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG` | `always_on`, `always_off`, `traceidratio`, `parentbased_traceidratio` with the ratio in `_ARG` |
| `OTEL_BSP_*`, `OTEL_BLRP_*` | batch sizes and delays of the span and log processors |

Transport is chosen in code: `.with_tonic()` is gRPC, `.with_http()` is OTLP over HTTP on
port 4318. `Resource::builder()` reads `OTEL_SERVICE_NAME` and `OTEL_RESOURCE_ATTRIBUTES` on
its own; `with_service_name` overrides the former, which is why the example only sets it
when the variable is absent.

### Where to point it

- **Look at the raw data.** `examples/otlp/collector.yaml` runs the OpenTelemetry Collector
  with a debug exporter that prints every span and log record it receives.

  ```sh
  docker run --rm -d --name guideme-otel -p 4317:4317 -p 4318:4318 \
    -v "$PWD/examples/otlp:/conf:ro" \
    otel/opentelemetry-collector-contrib:latest --config=/conf/collector.yaml
  docker logs -f guideme-otel
  ```

- **A UI on your laptop.** `grafana/otel-lgtm` is one container with an OTLP receiver,
  Tempo for traces, Loki for logs and Grafana in front, and it links a log record to its
  trace through the ids described above.

  ```sh
  docker run --rm -d --name lgtm -p 3000:3000 -p 4317:4317 -p 4318:4318 grafana/otel-lgtm
  ```

- **A vendor.** Set `OTEL_EXPORTER_OTLP_ENDPOINT` to their OTLP endpoint and put the API
  key in `OTEL_EXPORTER_OTLP_HEADERS`; their documentation names the header. Products with
  an LLM observability view pick the ask span up as a model call through its `gen_ai.*`
  attributes.

- **Metrics without instrumenting.** The collector's `spanmetrics` connector turns the
  spans into request, error and duration series, grouped by any attribute. Token cost per
  model is `sum(gen_ai.usage.input_tokens) by (gen_ai.response.model)` over the ask spans.

### What arrives

Captured from `otel/opentelemetry-collector-contrib` with the debug exporter, running
`examples/otlp` against the live API. One three-question ask:

```
Span #1
    Trace ID       : b1485f61703c4b5d88a67c2e036c6950
    Parent ID      :
    Name           : guideme.ask
    Kind           : Client
    Status code    : Unset
Attributes:
     -> gen_ai.provider.name: Str(typesafe)
     -> gen_ai.operation.name: Str(ask)
     -> gen_ai.request.model: Str(jev-latest)
     -> server.address: Str(api.typesafe.ai)
     -> server.port: Int(443)
     -> guideme.questions: Int(3)
     -> guideme.state.bytes: Int(122)
     -> gen_ai.response.model: Str(jev-1.13.0)
     -> gen_ai.usage.input_tokens: Int(422)
     -> gen_ai.usage.output_tokens: Int(71)

Span #0
    Trace ID       : b1485f61703c4b5d88a67c2e036c6950
    Parent ID      : 4d585471a3ac9a6d
    Name           : POST /v1/systemone
    Kind           : Client
    Status code    : Unset
Attributes:
     -> http.request.method: Str(POST)
     -> server.address: Str(api.typesafe.ai)
     -> server.port: Int(443)
     -> url.full: Str(https://api.typesafe.ai/v1/systemone)
     -> url.template: Str(/v1/systemone)
     -> http.response.status_code: Int(200)
```

Its answers, as log records on the logs pipeline, each pointing back at the ask span:

```
InstrumentationScope guideme
LogRecord #1
SeverityText: INFO
EventName: guideme.answer
Body: Str(q1 score: level 1)
Attributes:
     -> guideme.question: Str(q1)
     -> guideme.kind: Str(score)
     -> guideme.outcome: Int(1)
     -> guideme.value: Double(1.44)
     -> guideme.confidence: Double(0.33)
     -> guideme.unsure: Bool(false)
     -> guideme.min_confidence: Double(0)
Trace ID: b1485f61703c4b5d88a67c2e036c6950
Span ID: 4d585471a3ac9a6d
```

A failed ask arrives with the status set and the error typed:

```
    Name           : guideme.ask
    Status code    : Error
    Status message : unsure answer for question q0: 0.41 against threshold 0.999
     -> error.type: Str(unsure)
```

## Useful queries

- Cost per call: sum `gen_ai.usage.input_tokens` by `gen_ai.response.model`.
- Provider trouble: HTTP spans with `http.request.resend_count` set, or `guideme.retry`
  records per minute. Ask spans with `error.type` in `rate_limited`, `overloaded`,
  `transport` are the ones that gave up.
- Threshold tuning: on `guideme.answer`, the rate of `guideme.unsure=true` per
  `guideme.question`, next to the `guideme.min_confidence` recorded beside it. A band that
  is too wide shows up as reviewers drowning; too narrow shows up as wrong routes.
- Drift: `error.type=protocol` means the API's shape changed.
- One customer's ticket: the trace id links the ask span, its HTTP attempts and every
  answer, so a single trace view explains one decision end to end.
