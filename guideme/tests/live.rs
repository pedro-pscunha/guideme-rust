#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]
//! Opt-in contract check against the real API. Runs only with `TYPESAFE_API_KEY` set:
//! `cargo nextest run -p guideme --test live --run-ignored ignored-only`.

use guideme::api::{Answer, Client, Question, Request};
use guideme::{ApiKey, Model, State};

#[tokio::test]
#[ignore = "hits the live TypeSafe API; needs TYPESAFE_API_KEY"]
async fn docs_examples_still_parse_against_the_live_api() -> Result<(), Box<dyn std::error::Error>>
{
    let Ok(key) = std::env::var("TYPESAFE_API_KEY") else {
        eprintln!("TYPESAFE_API_KEY not set; skipping");
        return Ok(());
    };
    let client = Client::new(ApiKey::from(key))?;
    let request = Request {
        state: State::from("Help! My payouts have been failing for 3 days."),
        model: Model::latest(),
        questions: [
            (
                "is_urgent".into(),
                Question::Noul {
                    instructions: "Does this convey urgency?".into(),
                    criteria: None,
                },
            ),
            (
                "department".into(),
                Question::Choice {
                    instructions: "Which team should handle this?".into(),
                    criteria: [
                        (
                            "billing".to_owned(),
                            Some("Payments, invoicing, refunds".to_owned()),
                        ),
                        (
                            "technical".to_owned(),
                            Some("Bugs, outages, integrations".to_owned()),
                        ),
                        (
                            "sales".to_owned(),
                            Some("Pricing, upgrades, new accounts".to_owned()),
                        ),
                    ]
                    .into(),
                },
            ),
            (
                "frustration".into(),
                Question::Score {
                    instructions: "How frustrated is the customer?".into(),
                    criteria: vec!["Calm".into(), "Frustrated".into(), "Very angry".into()],
                },
            ),
        ]
        .into(),
    };
    let response = client.evaluate(&request).await?;
    assert!(matches!(response.answers["is_urgent"], Answer::Noul { .. }));
    assert!(matches!(
        response.answers["department"],
        Answer::Choice { .. }
    ));
    assert!(matches!(
        response.answers["frustration"],
        Answer::Score { .. }
    ));
    assert!(response.usage.input_tokens > 0);
    Ok(())
}
