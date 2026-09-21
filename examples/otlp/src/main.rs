//! Console logs and OTLP traces for guideme, against the live TypeSafe API.
//!
//! ```sh
//! export TYPESAFE_API_KEY=...
//! export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317   # optional, this is the default
//! RUST_LOG=guideme=info cargo run
//! ```
//!
//! Without a collector listening the program still runs and still logs; the exporter
//! retries in the background and reports the failure on shutdown.

use std::error::Error;

use guideme::{Choice, Guide, Levels, Policy, choose, noul, score};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
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

fn telemetry() -> Result<SdkTracerProvider, Box<dyn Error>> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name("support-triage")
                .build(),
        )
        .build();

    // Both layers read the same filter. RUST_LOG wins; this is the fallback.
    let filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,guideme=info"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_filter(filter()))
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(provider.tracer("guideme"))
                .with_filter(filter()),
        )
        .init();

    Ok(provider)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let provider = telemetry()?;
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
    // the span carries an error field.
    let strict = guide.with_policy(Policy::new().min_confidence(0.999))?;
    match strict
        .ask(score::<Frustration>("How frustrated is the customer?"), ticket)
        .await
    {
        Ok(level) => println!("strict frustration: {level:?}"),
        Err(e) => println!("strict frustration failed as expected: {e}"),
    }

    // Flush before exit, otherwise the batch exporter drops what it is holding.
    provider.shutdown()?;
    Ok(())
}
