#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use guideme::{Guide, Key, choose_among, noul};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct Captured {
    span_fields: BTreeMap<String, String>,
    events: Vec<BTreeMap<String, String>>,
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Captured>>);

struct Collect<'a>(&'a mut BTreeMap<String, String>);

impl Visit for Collect<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
        if attrs.metadata().name() == "guideme.ask" {
            attrs.record(&mut Collect(&mut self.0.lock().unwrap().span_fields));
        }
    }
    fn on_record(&self, _id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        values.record(&mut Collect(&mut self.0.lock().unwrap().span_fields));
    }
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().name() == "guideme.answer" {
            let mut fields = BTreeMap::new();
            event.record(&mut Collect(&mut fields));
            self.0.lock().unwrap().events.push(fields);
        }
    }
}

const REPLY: &str = r#"{"model":"jev-1.13.0","answers":{"q0":{"type":"noul","noul":0.95},"q1":{"type":"choice","choice":"billing","probabilities":{"billing":0.88,"sales":0.12},"confidence":0.81}},"usage":{"input_tokens":296,"output_tokens":20}}"#;

#[tokio::test]
async fn the_ask_span_carries_typed_fields_and_never_the_state_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let capture = Capture::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(capture.clone()));

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(REPLY))
        .mount(&server)
        .await;
    let guide = Guide::builder()
        .api_key("k".into())
        .base_url(server.uri())
        .build()?;

    let (yes, team) = guide
        .ask(
            (
                noul("Urgent?"),
                choose_among("Team?", [("billing", None), ("sales", None)]),
            ),
            "secret state text",
        )
        .await?;
    assert!(yes);
    assert_eq!(team, Key("billing".into()));

    let captured = capture.0.lock().unwrap();
    let f = &captured.span_fields;
    assert_eq!(f["model.requested"], "jev-latest");
    assert_eq!(f["model.answered"], "jev-1.13.0");
    assert_eq!(f["questions"], "2");
    assert_eq!(f["usage.input_tokens"], "296");
    assert_eq!(f["usage.output_tokens"], "20");
    assert_eq!(f["retries"], "0");
    assert!(f.contains_key("elapsed_ms"));
    assert!(!f.contains_key("state"));
    assert!(!f.values().any(|v| v.contains("secret state text")));

    assert_eq!(captured.events.len(), 2);
    let e = &captured.events[0];
    assert_eq!(e["question"], "q0");
    assert_eq!(e["kind"], "noul");
    assert_eq!(e["outcome"], "yes");
    assert_eq!(e["probability"], "0.95");
    assert_eq!(e["unsure"], "false");
    assert_eq!(e["yes_above"], "0.5");
    assert_eq!(e["min_confidence"], "0");
    let e = &captured.events[1];
    assert_eq!(e["question"], "q1");
    assert_eq!(e["kind"], "choice");
    assert_eq!(e["outcome"], "billing");
    assert_eq!(e["confidence"], "0.81");
    assert!(!e.contains_key("probability"));
    Ok(())
}

#[tokio::test]
async fn record_state_opts_in_to_recording_the_state() -> Result<(), Box<dyn std::error::Error>> {
    let capture = Capture::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(capture.clone()));
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(REPLY))
        .mount(&server)
        .await;
    let guide = Guide::builder()
        .api_key("k".into())
        .base_url(server.uri())
        .record_state(true)
        .build()?;
    guide.ask(noul("Urgent?"), "visible state").await?;
    assert_eq!(
        capture.0.lock().unwrap().span_fields["state"],
        "\"visible state\""
    );
    Ok(())
}
