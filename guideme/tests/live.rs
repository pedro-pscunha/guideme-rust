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

#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Department {
    /// Payments, invoicing, refunds
    Billing,
    /// Bugs, outages, integrations
    Technical,
    /// Pricing, upgrades, new accounts
    #[guide(fallback)]
    Sales,
}

#[derive(guideme::Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Frustration {
    /// Calm and polite
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}

#[tokio::test]
#[ignore = "hits the live TypeSafe API; needs TYPESAFE_API_KEY"]
async fn the_guide_surface_works_end_to_end_against_the_live_api()
-> Result<(), Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;

    use guideme::{Guide, Policy, Verdict, choose, choose_among, noul, score, score_levels};

    if std::env::var("TYPESAFE_API_KEY").is_err() {
        eprintln!("TYPESAFE_API_KEY not set; skipping");
        return Ok(());
    }
    let guide = Guide::from_env()?;
    let ticket =
        "Help! My payouts have been failing for 3 days and nobody answers. I was charged twice.";

    // if
    let urgent = guide.ask(noul("Does this convey urgency?"), ticket).await?;
    eprintln!("urgent = {urgent}");
    assert!(urgent);

    // match
    let dept = guide
        .ask(
            choose::<Department>("Which team should handle this?"),
            ticket,
        )
        .await?;
    eprintln!("dept = {dept:?}");
    match dept {
        Department::Billing | Department::Technical | Department::Sales => {}
    }
    assert_eq!(dept, Department::Billing);

    // compare
    let mood = guide
        .ask(
            score::<Frustration>("How frustrated is the customer?"),
            ticket,
        )
        .await?;
    eprintln!("mood = {mood:?}");
    assert!(mood >= Frustration::Frustrated);

    // batch: tuple + map + runtime rubrics + detail, one request
    let labels = BTreeMap::from([
        ("billing", noul("Is this ticket about billing?")),
        ("bot", noul("Was this written by a bot?")),
    ]);
    let (v, ranked, scored, owner, urgency, flags) = guide
        .ask(
            (
                noul("Is the customer asking for a refund?")
                    .yes_above(0.7)
                    .no_below(0.3)
                    .detail(),
                choose::<Department>("Which team should handle this?")
                    .min_confidence(0.5)
                    .detail(),
                score::<Frustration>("How frustrated is the customer?").detail(),
                choose_among(
                    "Who should own this?",
                    [
                        ("ana", Some("payments engineer")),
                        ("bruno", Some("sales rep")),
                    ],
                ),
                score_levels("How urgent?", ["can wait", "this week", "today"]).detail(),
                labels,
            ),
            ticket,
        )
        .await?;
    eprintln!("refund verdict = {v:?}");
    eprintln!("ranked = {ranked:?}");
    eprintln!("scored = {scored:?}");
    eprintln!("owner = {owner:?}");
    eprintln!("urgency = {urgency:?}");
    eprintln!("flags = {flags:?}");
    assert!(matches!(
        v,
        Verdict::Yes(_) | Verdict::No(_) | Verdict::Unsure(_)
    ));
    assert_eq!(ranked.probabilities.len(), 3);
    assert_eq!(scored.distribution.len(), 3);
    assert!((0.0..=2.0).contains(&scored.value));
    assert_eq!(owner.0, "ana");
    assert!(urgency.level.0 >= 1);
    assert!(flags["billing"]);
    assert!(!flags["bot"]);

    // a scoped strict policy validates and still answers
    let strict = guide.with_policy(Policy::new().min_confidence(0.99))?;
    let fallback = strict
        .ask(
            choose::<Department>("Which team should handle this?"),
            ticket,
        )
        .await;
    eprintln!("strict = {fallback:?}");
    assert!(fallback.is_ok());

    let models = guide.models().await?;
    eprintln!("models = {models:?}");
    assert!(models.iter().any(|m| m.name == "jev-latest"));
    Ok(())
}
