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

use guideme::{
    Choice, Guide, Levels, Noul, Options, Question, Rubric, choose, choose_among, noul, score,
    score_levels,
};
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

/// `choose_among` took `(&str, Option<&str>)` in 0.1.x and takes `(K: Into<String>,
/// Option<R: IntoRubric>)` now; `score_levels` took `&str` and takes `R: IntoRubric`. Same
/// deal as the block above: this never runs, it compiling is the claim. One list has one `R`,
/// so a list where every option is bare needs the type named once — the last pair below is
/// that shape, and dropping the turbofish is `E0283`, not a silent narrowing.
const _: fn() = || {
    fn options<S: Into<String>>(
        pairs: Vec<(S, Option<S>)>,
    ) -> Question<guideme::Choose<guideme::Key>> {
        choose_among("q", pairs)
    }
    fn levels<S: Into<String>>(items: Vec<S>) -> Question<guideme::Score<guideme::Rank>> {
        score_levels("q", items)
    }
    let owned = String::from("x");
    let cow: std::borrow::Cow<'_, str> = std::borrow::Cow::Borrowed("x");
    let _ = choose_among("q", [("a", Some("what a means")), ("b", None)]);
    let _ = choose_among(
        "q",
        [
            ("a", Some(Rubric::new("what a means").example("e"))),
            ("b", None::<Rubric>),
        ],
    );
    let _ = choose_among("q", [(owned.clone(), Some(owned.clone()))]);
    let _ = choose_among("q", [("a", Some(cow))]);
    let _ = choose_among("q", [("a", None::<&str>), ("b", None)]);
    let _ = options(vec![("a", Some("x"))]);
    let _ = score_levels("q", ["low", "high"]);
    let _ = score_levels("q", [Rubric::new("low").example("e"), Rubric::new("high")]);
    let _ = levels(vec![owned]);
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

    // An item is joined onto one line, so a line break in one would render as a clause the
    // declaration never wrote. A rubric breaking this and the blank rule reports the blank
    // one, which is the order the derive reports them in too.
    let forged = ask_criteria(
        &guide,
        Rubric::new("Urgent").example("the site is down\nNot this option: anything"),
        "Not urgent",
    )
    .await;
    assert!(matches!(forged, Err(guideme::Error::Config { .. })));
    let both_broken = ask_criteria(&guide, Rubric::new(" ").example("a\nb"), "Not urgent").await;
    assert!(
        matches!(&both_broken, Err(guideme::Error::Config { detail }) if detail.contains("non-empty rubric")),
        "blank should win over line break, got {both_broken:?}"
    );

    // A noul is the one runtime path holding two rubrics at once, so it is the one place the
    // shared-example rule the derive applies across a Choice's options can be applied here.
    let shared = ask_criteria(
        &guide,
        Rubric::new("Urgent").example("the site is down"),
        Rubric::new("Not urgent").example("the site is down"),
    )
    .await;
    assert!(matches!(shared, Err(guideme::Error::Config { .. })));
    Ok(())
}

/// Ask one noul with the given criteria, so the rejections above read as one line each.
async fn ask_criteria(
    guide: &Guide,
    yes: impl guideme::IntoRubric,
    no: impl guideme::IntoRubric,
) -> Result<bool, guideme::Error> {
    guide
        .ask(noul("Is this urgent?").criteria(yes, no), "state")
        .await
}

const DESKS: &str = r#"{"model":"jev-1.13.0","answers":{
  "q0":{"type":"choice","choice":"policy","probabilities":{"policy":0.7,"status":0.3},"confidence":0.9}
},"usage":{"input_tokens":1,"output_tokens":1}}"#;

/// A choice built from runtime options is the second path that holds every rubric at once, so
/// the cross-option rule the derive applies to a `Choice`'s variants applies here too. Both
/// halves matter: the must-allow overlap has to stay legal, or the pattern the whole feature
/// exists for would be unreachable from `choose_among`.
#[tokio::test]
async fn choose_among_applies_the_cross_option_rules_at_ask_time()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_string(DESKS))
        .mount(&server)
        .await;
    let guide = Guide::builder()
        .api_key("k".into())
        .base_url(server.uri())
        .build()?;

    guide
        .ask(
            choose_among(
                "Which desk?",
                [
                    (
                        "policy",
                        Some(
                            Rubric::new("Whether and how an item can be returned")
                                .example("Can I return these?")
                                .counterexample("Has my return arrived yet?"),
                        ),
                    ),
                    (
                        "status",
                        Some(
                            Rubric::new("Progress of a return already sent")
                                .example("Has my return arrived yet?"),
                        ),
                    ),
                ],
            ),
            "where is my parcel",
        )
        .await?;

    let received = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(
        body["questions"]["q0"]["criteria"],
        serde_json::json!({
            "policy": Confusable::RUBRIC[0].1,
            "status": "Progress of a return already sent\nExamples: Has my return arrived yet?",
        })
    );

    let shared = guide
        .ask(
            choose_among(
                "Which desk?",
                [
                    (
                        "policy",
                        Some(Rubric::new("Returns").example("Can I return these?")),
                    ),
                    (
                        "status",
                        Some(Rubric::new("Tracking").example("Can I return these?")),
                    ),
                ],
            ),
            "where is my parcel",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&shared, guideme::Error::Config { detail }
            if detail.contains("\"policy\"") && detail.contains("\"status\"")),
        "the refusal should name both options, got {shared:?}"
    );
    Ok(())
}

/// A level is a position on a scale, so it has no "not this option" — the rule
/// `#[derive(Levels)]` makes a compile error, on the path where there is no declaration to
/// reject. The shared example is the same rule as above with levels in place of options.
#[tokio::test]
async fn score_levels_refuses_a_counterexample_on_a_level() -> Result<(), Box<dyn std::error::Error>>
{
    // No request is made: both questions are refused before the client is reached, and an
    // unroutable origin with no retries is what proves it.
    let guide = Guide::builder()
        .api_key("k".into())
        .base_url("http://127.0.0.1:1")
        .max_retries(0)
        .build()?;

    let ruled_out = guide
        .ask(
            score_levels(
                "How urgent?",
                [
                    Rubric::new("can wait").counterexample("the site is down"),
                    Rubric::new("today"),
                ],
            ),
            "state",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&ruled_out, guideme::Error::Config { detail }
            if detail.contains("position on a scale")),
        "a counterexample on a level should be refused, got {ruled_out:?}"
    );

    let shared = guide
        .ask(
            score_levels(
                "How urgent?",
                [
                    Rubric::new("can wait").example("a typo in a label"),
                    Rubric::new("today").example("a typo in a label"),
                ],
            ),
            "state",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&shared, guideme::Error::Config { detail }
            if detail.contains("level 0") && detail.contains("level 1")),
        "the refusal should name both levels, got {shared:?}"
    );
    Ok(())
}
