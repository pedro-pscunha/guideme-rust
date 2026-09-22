#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]
//! The rendered rubric is a cross-SDK contract item, so the exact bytes are asserted here and
//! in `spec/vectors/rubric.json`. `docs/contract.md` states the algorithm.

use guideme::{Choice, Guide, Levels, Options, choose, score};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// One variant per row of the golden table in `docs/contract.md`.
#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Golden {
    /// Payments, invoicing, refunds
    Bare,
    /// Bugs, outages, integrations
    #[guide(example = "502 on every request")]
    OneExample,
    /// Payments, invoicing, refunds
    #[guide(example = "My card was charged twice", example = "Where is my refund?")]
    TwoExamples,
    /// Payments, invoicing, refunds
    #[guide(example = "My card was charged twice")]
    #[guide(counterexample = "The dashboard is down")]
    Both,
    /// Payments, invoicing, refunds
    #[guide(counterexample = "The dashboard is down")]
    OnlyCounterexample,
}

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Severity {
    /// No impact to functionality
    #[guide(example = "typo in a label", example = "misaligned icon")]
    Cosmetic,
    /// No workaround exists
    Blocking,
}

#[test]
fn every_golden_row_renders_byte_for_byte() {
    assert_eq!(
        Golden::RUBRIC,
        &[
            // No examples and no counterexamples: the rubric itself, unchanged from 0.1.0.
            ("bare", Some("Payments, invoicing, refunds")),
            (
                "one_example",
                Some("Bugs, outages, integrations\nExamples: 502 on every request")
            ),
            (
                "two_examples",
                Some(
                    "Payments, invoicing, refunds\nExamples: My card was charged twice; Where is my refund?"
                )
            ),
            (
                "both",
                Some(
                    "Payments, invoicing, refunds\nExamples: My card was charged twice\nNot this option: The dashboard is down"
                )
            ),
            (
                "only_counterexample",
                Some("Payments, invoicing, refunds\nNot this option: The dashboard is down")
            ),
        ]
    );
    assert_eq!(
        Severity::LEVELS,
        &[
            "No impact to functionality\nExamples: typo in a label; misaligned icon",
            "No workaround exists",
        ]
    );
}

const REPLY: &str = r#"{"model":"jev-1.13.0","answers":{
  "q0":{"type":"choice","choice":"bare","probabilities":{"bare":0.6,"one_example":0.1,"two_examples":0.1,"both":0.1,"only_counterexample":0.1},"confidence":0.9},
  "q1":{"type":"score","score":0.2,"legend":{"0":"a","1":"b"},"probabilities":{"0":0.8,"1":0.2},"confidence":0.9}
},"usage":{"input_tokens":1,"output_tokens":1}}"#;

#[tokio::test]
async fn the_rendered_rubric_is_what_reaches_the_wire() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_string(REPLY))
        .mount(&server)
        .await;
    let guide = Guide::builder()
        .api_key("k".into())
        .base_url(server.uri())
        .build()?;
    guide
        .ask(
            (
                choose::<Golden>("Which team should handle this?"),
                score::<Severity>("How bad is it?"),
            ),
            "the export button crashes",
        )
        .await?;

    let received = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(
        body["questions"]["q0"]["criteria"],
        serde_json::json!({
            "bare": "Payments, invoicing, refunds",
            "one_example": "Bugs, outages, integrations\nExamples: 502 on every request",
            "two_examples": "Payments, invoicing, refunds\nExamples: My card was charged twice; Where is my refund?",
            "both": "Payments, invoicing, refunds\nExamples: My card was charged twice\nNot this option: The dashboard is down",
            "only_counterexample": "Payments, invoicing, refunds\nNot this option: The dashboard is down",
        })
    );
    assert_eq!(
        body["questions"]["q1"]["criteria"],
        serde_json::json!([
            "No impact to functionality\nExamples: typo in a label; misaligned icon",
            "No workaround exists",
        ])
    );
    Ok(())
}
