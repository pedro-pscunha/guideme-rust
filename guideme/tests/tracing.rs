#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use guideme::{Guide, Key, choose_among, noul};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

type Fields = BTreeMap<String, String>;

struct Span {
    name: String,
    parent: Option<String>,
    fields: Fields,
}

#[derive(Default)]
struct Captured {
    index: HashMap<u64, usize>,
    spans: Vec<Span>,
    events: Vec<(String, Level, Fields)>,
}

impl Captured {
    fn span(&self, name: &str) -> &Fields {
        &self.spans_named(name)[0].fields
    }
    fn spans_named(&self, name: &str) -> Vec<&Span> {
        self.spans.iter().filter(|s| s.name == name).collect()
    }
    fn events_named(&self, name: &str) -> Vec<(&Level, &Fields)> {
        self.events
            .iter()
            .filter(|(n, _, _)| n == name)
            .map(|(_, l, f)| (l, f))
            .collect()
    }
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Captured>>);

struct Collect<'a>(&'a mut Fields);

impl Visit for Collect<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
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

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Fields::new();
        attrs.record(&mut Collect(&mut fields));
        let parent = ctx
            .span(id)
            .and_then(|s| s.parent())
            .map(|p| p.name().to_owned());
        let mut captured = self.0.lock().unwrap();
        let at = captured.spans.len();
        captured.index.insert(id.into_u64(), at);
        captured.spans.push(Span {
            name: attrs.metadata().name().to_owned(),
            parent,
            fields,
        });
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let mut captured = self.0.lock().unwrap();
        let at = captured.index[&id.into_u64()];
        values.record(&mut Collect(&mut captured.spans[at].fields));
    }
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields::new();
        event.record(&mut Collect(&mut fields));
        self.0.lock().unwrap().events.push((
            event.metadata().name().to_owned(),
            *event.metadata().level(),
            fields,
        ));
    }
}

const REPLY: &str = r#"{"model":"jev-1.13.0","answers":{"q0":{"type":"noul","noul":0.95},"q1":{"type":"choice","choice":"billing","probabilities":{"billing":0.88,"sales":0.12},"confidence":0.81}},"usage":{"input_tokens":296,"output_tokens":20}}"#;

fn install(capture: &Capture) -> tracing::subscriber::DefaultGuard {
    tracing::subscriber::set_default(tracing_subscriber::registry().with(capture.clone()))
}

#[tokio::test]
async fn one_ask_is_one_span_with_an_http_span_per_attempt()
-> Result<(), Box<dyn std::error::Error>> {
    let capture = Capture::default();
    let _guard = install(&capture);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
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

    let ask = captured.span("guideme.ask");
    assert_eq!(ask["otel.kind"], "client");
    assert_eq!(ask["gen_ai.provider.name"], "typesafe");
    assert_eq!(ask["gen_ai.operation.name"], "ask");
    assert_eq!(ask["gen_ai.request.model"], "jev-latest");
    assert_eq!(ask["gen_ai.response.model"], "jev-1.13.0");
    assert_eq!(ask["gen_ai.usage.input_tokens"], "296");
    assert_eq!(ask["gen_ai.usage.output_tokens"], "20");
    assert_eq!(ask["server.address"], "127.0.0.1");
    assert_eq!(ask["server.port"], server.address().port().to_string());
    assert_eq!(ask["guideme.questions"], "2");
    assert!(!ask.contains_key("guideme.state"));
    assert!(!ask.contains_key("error.type"));
    assert!(!ask.contains_key("otel.status_code"));
    assert!(
        !captured
            .spans
            .iter()
            .any(|s| s.fields.values().any(|v| v.contains("secret state text")))
    );
    assert_eq!(captured.spans_named("guideme.ask")[0].parent, None);

    let attempts = captured.spans_named("POST /v1/systemone");
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts
            .iter()
            .all(|a| a.parent.as_deref() == Some("guideme.ask"))
    );
    let throttled = &attempts[0].fields;
    assert_eq!(throttled["otel.kind"], "client");
    assert_eq!(throttled["http.request.method"], "POST");
    assert_eq!(
        throttled["url.full"],
        format!("{}/v1/systemone", server.uri())
    );
    assert_eq!(throttled["url.template"], "/v1/systemone");
    assert_eq!(throttled["http.response.status_code"], "429");
    assert_eq!(throttled["error.type"], "429");
    assert_eq!(throttled["otel.status_code"], "ERROR");
    assert!(!throttled.contains_key("http.request.resend_count"));
    let succeeded = &attempts[1].fields;
    assert_eq!(succeeded["http.request.resend_count"], "1");
    assert_eq!(succeeded["http.response.status_code"], "200");
    assert!(!succeeded.contains_key("error.type"));

    let retries = captured.events_named("guideme.retry");
    assert_eq!(retries.len(), 1);
    let (level, retry) = retries[0];
    assert_eq!(*level, Level::WARN);
    assert_eq!(retry["http.response.status_code"], "429");
    assert_eq!(retry["guideme.retry.attempt"], "1");
    assert_eq!(retry["guideme.retry.delay_ms"], "0");

    let answers = captured.events_named("guideme.answer");
    assert_eq!(answers.len(), 2);
    let (level, e) = answers[0];
    assert_eq!(*level, Level::INFO);
    assert_eq!(e["guideme.question"], "q0");
    assert_eq!(e["guideme.kind"], "noul");
    assert_eq!(e["guideme.outcome"], "yes");
    assert_eq!(e["guideme.probability"], "0.95");
    assert_eq!(e["guideme.unsure"], "false");
    assert_eq!(e["guideme.yes_above"], "0.5");
    assert_eq!(e["guideme.min_confidence"], "0");
    let (_, e) = answers[1];
    assert_eq!(e["guideme.question"], "q1");
    assert_eq!(e["guideme.kind"], "choice");
    assert_eq!(e["guideme.outcome"], "billing");
    assert_eq!(e["guideme.confidence"], "0.81");
    assert!(!e.contains_key("guideme.probability"));
    Ok(())
}

#[tokio::test]
async fn record_state_opts_in_to_recording_the_state() -> Result<(), Box<dyn std::error::Error>> {
    let capture = Capture::default();
    let _guard = install(&capture);
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
        capture.0.lock().unwrap().span("guideme.ask")["guideme.state"],
        "\"visible state\""
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_ask_marks_the_span_with_a_stable_error_type()
-> Result<(), Box<dyn std::error::Error>> {
    let capture = Capture::default();
    let _guard = install(&capture);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(REPLY))
        .mount(&server)
        .await;
    let guide = Guide::builder()
        .api_key("k".into())
        .base_url(server.uri())
        .build()?;

    let err = guide
        .ask(noul("Urgent?").yes_above(0.99), "state")
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "unsure");

    let captured = capture.0.lock().unwrap();
    let ask = captured.span("guideme.ask");
    assert_eq!(ask["error.type"], "unsure");
    assert_eq!(ask["otel.status_code"], "ERROR");
    assert_eq!(ask["otel.status_description"], err.to_string());
    assert_eq!(ask["gen_ai.response.model"], "jev-1.13.0");
    assert!(
        !captured
            .span("POST /v1/systemone")
            .contains_key("error.type")
    );
    Ok(())
}
