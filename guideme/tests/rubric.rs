#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]
//! The rendered rubric is a cross-SDK contract item, so the exact bytes are asserted here and
//! in `spec/vectors/rubric.json`. `docs/contract.md` states the algorithm.
//!
//! There are two renderers: the proc macro's, which runs at expansion time because
//! `Options::RUBRIC` is a `const`, and `Rubric`'s, for the positions that are not an enum.
//! Every golden row below is asserted against both — the derive's through the request body it
//! produces, `Rubric`'s directly — which is what keeps them one algorithm. `spec/` cannot do
//! this job: its vector is generated from the derive, so it re-states whatever the renderer
//! currently does. `ROWS` is written by hand, which is the point.

use guideme::{Choice, Guide, Levels, Noul, Options, Question, Rubric, choose, noul, score};
use proptest::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The golden rows that carry no counterexample.
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
}

/// The rows that do, plus the one that pins declaration order: neither clause is in sorted
/// order, so a renderer that sorted would not reproduce it. A second enum because the golden
/// rows deliberately reuse a string that no single declaration may use twice.
#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum GoldenCounter {
    /// Payments, invoicing, refunds
    #[guide(example = "My card was charged twice")]
    #[guide(counterexample = "The dashboard is down")]
    Both,
    /// Payments, invoicing, refunds
    #[guide(counterexample = "The dashboard is down")]
    OnlyCounterexample,
    /// Payments, invoicing, refunds
    #[guide(example = "Why was I billed twice?", example = "Cancel and refund me")]
    #[guide(
        counterexample = "The status page is red",
        counterexample = "Are you hiring?"
    )]
    DeclarationOrder,
}

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Severity {
    /// No impact to functionality
    #[guide(example = "typo in a label", example = "misaligned icon")]
    Cosmetic,
    /// No workaround exists
    Blocking,
}

/// The one overlap the rules must keep legal: the same string as an example of one option and
/// a counterexample of another. This compiling is the assertion — the derive rejects an example
/// shared by two options, and over-firing to this shape would break the pattern the whole
/// feature exists to serve.
#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Confusable {
    /// Whether and how an item can be returned
    #[guide(example = "Can I return these?")]
    #[guide(counterexample = "Has my return arrived yet?")]
    Policy,
    /// Progress of a return already sent
    #[guide(example = "Has my return arrived yet?")]
    #[guide(counterexample = "Can I return these?")]
    Status,
}

/// `criteria` took `impl Into<String>` in 0.1.0 and takes `impl IntoRubric` now. This block
/// never runs; it compiling is the whole claim that the change breaks no caller, so deleting a
/// line of it silently narrows the public API. The last one is the shape no set of `From`
/// conversions can cover, because a generic bound is not a concrete type.
const _: fn() = || {
    fn passthrough<S: Into<String>>(yes: S, no: S) -> Question<Noul> {
        noul("q").criteria(yes, no)
    }
    let mut owned = String::from("x");
    let cow: std::borrow::Cow<'_, str> = std::borrow::Cow::Borrowed("x");
    let boxed: Box<str> = "x".into();
    let _ = noul("q").criteria("literal", format!("{owned}!"));
    let _ = noul("q").criteria(owned.clone(), &owned);
    let _ = noul("q").criteria(cow, boxed);
    let _ = noul("q").criteria('y', owned.as_mut_str());
    let _ = noul("q").criteria(Rubric::new("y").example("e"), Rubric::new("n"));
    let _ = passthrough("a", "b");
};

/// `(what, examples, counterexamples, rendered)`, in the order the variants are declared
/// above. Written out by hand: this is the expected side of the golden assertion.
const ROWS: [(&str, &[&str], &[&str], &str); 8] = [
    // No examples and no counterexamples: the rubric itself, unchanged from 0.1.0.
    (
        "Payments, invoicing, refunds",
        &[],
        &[],
        "Payments, invoicing, refunds",
    ),
    (
        "Bugs, outages, integrations",
        &["502 on every request"],
        &[],
        "Bugs, outages, integrations\nExamples: 502 on every request",
    ),
    (
        "Payments, invoicing, refunds",
        &["My card was charged twice", "Where is my refund?"],
        &[],
        "Payments, invoicing, refunds\nExamples: My card was charged twice; Where is my refund?",
    ),
    (
        "Payments, invoicing, refunds",
        &["My card was charged twice"],
        &["The dashboard is down"],
        "Payments, invoicing, refunds\nExamples: My card was charged twice\nNot this option: The dashboard is down",
    ),
    (
        "Payments, invoicing, refunds",
        &[],
        &["The dashboard is down"],
        "Payments, invoicing, refunds\nNot this option: The dashboard is down",
    ),
    (
        "Payments, invoicing, refunds",
        &["Why was I billed twice?", "Cancel and refund me"],
        &["The status page is red", "Are you hiring?"],
        "Payments, invoicing, refunds\nExamples: Why was I billed twice?; Cancel and refund me\nNot this option: The status page is red; Are you hiring?",
    ),
    (
        "No impact to functionality",
        &["typo in a label", "misaligned icon"],
        &[],
        "No impact to functionality\nExamples: typo in a label; misaligned icon",
    ),
    ("No workaround exists", &[], &[], "No workaround exists"),
];

/// The same parts through the runtime renderer.
fn runtime(what: &str, examples: &[&str], counterexamples: &[&str]) -> String {
    let mut rubric = Rubric::new(what);
    for example in examples {
        rubric = rubric.example(*example);
    }
    for counterexample in counterexamples {
        rubric = rubric.counterexample(*counterexample);
    }
    rubric.render().expect("every golden row is a valid rubric")
}

/// The §9.5 pin. Two copies of the algorithm exist because `Options::RUBRIC` is a `const` and
/// cannot call a function, so only this keeps them one algorithm. It is deliberately not part
/// of the wire test: a pin that runs after a mock server has to start is a pin that stops
/// running the day the mock server breaks.
#[test]
fn the_two_renderers_agree_on_every_golden_row() {
    let derived: Vec<&str> = Golden::RUBRIC
        .iter()
        .chain(GoldenCounter::RUBRIC)
        .map(|&(_, rubric)| rubric.expect("every golden variant has a rubric"))
        .chain(Severity::LEVELS.iter().copied())
        .collect();
    assert_eq!(derived.len(), ROWS.len());
    for (from_derive, (what, examples, counterexamples, want)) in derived.iter().zip(ROWS) {
        assert_eq!(*from_derive, want, "the derive drifted on {what:?}");
        assert_eq!(
            runtime(what, examples, counterexamples),
            want,
            "Rubric drifted on {what:?}"
        );
    }
    assert_eq!(
        Confusable::RUBRIC[0].1,
        Some(
            "Whether and how an item can be returned\nExamples: Can I return these?\nNot this option: Has my return arrived yet?"
        )
    );
}

proptest! {
    /// The load-bearing invariant: a rubric with no parts is its own rendering, so a
    /// declaration written before examples existed puts the same bytes on the wire.
    #[test]
    fn a_rubric_with_no_parts_renders_to_itself(what in "(?s).{0,64}") {
        prop_assert_eq!(Rubric::new(&*what).render()?, what);
    }
}

const REPLY: &str = r#"{"model":"jev-1.13.0","answers":{
  "q0":{"type":"noul","noul":0.2},
  "q1":{"type":"choice","choice":"bare","probabilities":{"bare":0.6,"one_example":0.2,"two_examples":0.2},"confidence":0.9},
  "q2":{"type":"choice","choice":"both","probabilities":{"both":0.6,"only_counterexample":0.2,"declaration_order":0.2},"confidence":0.9},
  "q3":{"type":"score","score":0.2,"legend":{"0":"a","1":"b"},"probabilities":{"0":0.8,"1":0.2},"confidence":0.9}
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
                noul("Is this ticket urgent?").criteria(
                    Rubric::new("Broken now, no workaround")
                        .example("the checkout page is down")
                        .counterexample("a nightly job failed and the numbers are pulled by hand"),
                    Rubric::new("It can wait for the next working day"),
                ),
                choose::<Golden>("Which team should handle this?"),
                choose::<GoldenCounter>("And which desk?"),
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
            "true": "Broken now, no workaround\nExamples: the checkout page is down\nNot this option: a nightly job failed and the numbers are pulled by hand",
            "false": "It can wait for the next working day",
        })
    );
    assert_eq!(
        body["questions"]["q1"]["criteria"],
        serde_json::json!({
            "bare": ROWS[0].3,
            "one_example": ROWS[1].3,
            "two_examples": ROWS[2].3,
        })
    );
    assert_eq!(
        body["questions"]["q2"]["criteria"],
        serde_json::json!({
            "both": ROWS[3].3,
            "only_counterexample": ROWS[4].3,
            "declaration_order": ROWS[5].3,
        })
    );
    assert_eq!(
        body["questions"]["q3"]["criteria"],
        serde_json::json!([ROWS[6].3, ROWS[7].3])
    );

    // Examples attached to nothing never reach the wire: the same rule the derives enforce at
    // compile time, on the path where there is no declaration to reject. A blank description
    // carrying no parts is left alone, because that is what 0.1.0 accepted.
    let err = guide
        .ask(
            noul("Is this urgent?").criteria(Rubric::new(" ").example("the site is down"), "no"),
            "state",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, guideme::Error::Config { .. }));
    assert!(
        guide
            .ask(noul("Is this urgent?").criteria("", ""), "state")
            .await
            .is_ok()
    );
    Ok(())
}
