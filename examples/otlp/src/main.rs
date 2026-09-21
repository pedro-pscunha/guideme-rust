//! guideme against the live TypeSafe API, with console logs, OTLP traces and OTLP logs
//! that share trace and span ids.
//!
//! ```sh
//! docker run --rm -d --name guideme-otel -p 4317:4317 -p 4318:4318 \
//!   -v "$PWD/examples/otlp:/conf:ro" \
//!   otel/opentelemetry-collector-contrib:latest --config=/conf/collector.yaml
//!
//! export TYPESAFE_API_KEY=...
//! RUST_LOG=guideme=info cargo run          # from examples/otlp
//! docker logs guideme-otel                  # what arrived
//! ```
//!
//! The exporter is configured entirely through the standard OTLP environment variables
//! (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`,
//! `OTEL_RESOURCE_ATTRIBUTES`, `OTEL_TRACES_SAMPLER`), so pointing it at a vendor instead of
//! the local collector is a matter of setting those. Without a collector listening the
//! program still runs and still logs to the console; the exporters report the failure on
//! shutdown.

use std::error::Error;

use guideme::{Choice, Guide, Levels, Policy, choose, noul, score};
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

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Department {
    /// Payments, invoicing, refunds
    Billing,
    /// Bugs, outages, integrations
    Technical,
    /// Pricing, upgrades, new accounts
    #[guide(fallback)]
    Sales,
}

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Frustration {
    /// Calm and polite
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}

/// The two OTLP pipelines. Both must be shut down, or their last batch is lost.
struct Telemetry {
    traces: SdkTracerProvider,
    logs: SdkLoggerProvider,
}

impl Telemetry {
    /// Shuts both down whatever the first one says, then reports the first failure.
    fn shutdown(self) -> Result<(), Box<dyn Error>> {
        let traces = self.traces.shutdown();
        let logs = self.logs.shutdown();
        traces?;
        logs?;
        Ok(())
    }
}

fn telemetry() -> Result<Telemetry, Box<dyn Error>> {
    // `Resource::builder()` already reads OTEL_SERVICE_NAME and OTEL_RESOURCE_ATTRIBUTES;
    // the fallback only applies when the environment says nothing.
    let mut resource = Resource::builder();
    if std::env::var_os("OTEL_SERVICE_NAME").is_none() {
        resource = resource.with_service_name("support-triage");
    }
    let resource = resource.build();

    let traces = SdkTracerProvider::builder()
        .with_batch_exporter(SpanExporter::builder().with_tonic().build()?)
        .with_resource(resource.clone())
        .build();
    let logs = SdkLoggerProvider::builder()
        .with_batch_exporter(LogExporter::builder().with_tonic().build()?)
        .with_resource(resource)
        .build();

    // RUST_LOG wins; this is the fallback. Each layer gets its own copy so the console and
    // the exporters can be tuned apart.
    let filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,guideme=info"))
    };

    tracing_subscriber::registry()
        // Console: spans as context on each line, and a line when a span closes.
        .with(
            fmt::layer()
                .with_target(true)
                .with_span_events(fmt::format::FmtSpan::CLOSE)
                .with_filter(filter()),
        )
        // Traces: spans only. Events go to the logs pipeline below, so they are stored once
        // and linked by trace id rather than duplicated as span events. Drop the
        // `filter_fn` to get them as span events instead, for a backend without logs.
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(traces.tracer("guideme-otlp"))
                // Off by default here: the file path, thread and busy/idle attributes the
                // layer adds are process facts, not request facts, and the path leaks the
                // build machine. Turn any back on when you want it.
                .with_location(false)
                .with_threads(false)
                .with_tracked_inactivity(false)
                .with_filter(filter().and(filter_fn(|meta| meta.is_span()))),
        )
        // Logs: every event, carrying the trace id and span id of the span it was
        // emitted in. tracing-opentelemetry activates the OpenTelemetry context when a
        // span is entered, which is what the bridge reads.
        .with(OpenTelemetryTracingBridge::new(&logs).with_filter(filter()))
        .init();

    Ok(Telemetry { traces, logs })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let telemetry = telemetry()?;
    // Whatever happens in `run`, the exporters get their flush.
    let outcome = run().await;
    telemetry.shutdown()?;
    outcome
}

async fn run() -> Result<(), Box<dyn Error>> {
    let guide = Guide::from_env()?;
    let ticket = "Help! My payouts have been failing for 3 days and nobody answers. \
                  I was charged twice and I want the second charge back.";

    // One question: one span, one answer event.
    let urgent = guide
        .ask(noul("Does this convey urgency?"), ticket)
        .await?;
    println!("urgent: {urgent}");

    // Three questions in one request: one span, three answer events.
    let (dept, mood, refund) = guide
        .ask(
            (
                choose::<Department>("Which team should handle this?").min_confidence(0.6),
                score::<Frustration>("How frustrated is the customer?").detail(),
                noul("Is the customer asking for a refund?")
                    .yes_above(0.8)
                    .no_below(0.2)
                    .or(false),
            ),
            ticket,
        )
        .await?;
    println!("department: {dept:?}");
    println!("frustration: {:?} at {:.2}", mood.level, mood.value);
    println!("refund requested: {refund}");

    // A threshold nothing can satisfy. A score has no fallback member, so this fails and
    // the span carries `error.type = "unsure"` with an error status.
    let strict = guide.with_policy(Policy::new().min_confidence(0.999))?;
    match strict
        .ask(score::<Frustration>("How frustrated is the customer?"), ticket)
        .await
    {
        Ok(level) => println!("strict frustration: {level:?}"),
        Err(e) => println!("strict frustration failed as expected: {e}"),
    }
    Ok(())
}
