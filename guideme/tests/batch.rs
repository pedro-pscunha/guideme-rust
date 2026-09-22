#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::collections::BTreeMap;

use guideme::{
    Choice, Guide, Key, Levels, Policy, Rank, Scored, Verdict, choose, choose_among, noul, score,
    score_levels,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Department {
    /// Payments
    Billing,
    /// Bugs
    Technical,
    /// Pricing
    Sales,
}

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Team {
    /// a
    Billing,
    /// b
    #[guide(fallback)]
    Sales,
}

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Frustration {
    /// Calm
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}

const REPLY: &str = r#"{"model":"jev-1.13.0","answers":{
  "q0":{"type":"noul","noul":0.95},
  "q1":{"type":"choice","choice":"billing","probabilities":{"billing":0.5,"technical":0.3,"sales":0.2},"confidence":0.4},
  "q2":{"type":"score","score":1.05,"legend":{"0":"Calm","1":"Frustrated","2":"Very angry"},"probabilities":{"0":0.0,"1":0.95,"2":0.05},"confidence":0.92},
  "q3":{"type":"noul","noul":0.1},
  "q4":{"type":"noul","noul":0.9}
},"usage":{"input_tokens":300,"output_tokens":40}}"#;

const LADDER: &str = r#"{"model":"jev-1.13.0","answers":{
  "q0":{"type":"noul","noul":0.5},
  "q1":{"type":"choice","choice":"billing","probabilities":{"billing":0.6,"sales":0.4},"confidence":0.4},
  "q2":{"type":"choice","choice":"billing","probabilities":{"billing":0.6,"sales":0.4},"confidence":0.4},
  "q3":{"type":"choice","choice":"billing","probabilities":{"billing":0.6,"sales":0.4},"confidence":0.4},
  "q4":{"type":"choice","choice":"billing","probabilities":{"billing":0.6,"sales":0.4},"confidence":0.4},
  "q5":{"type":"score","score":0.4,"legend":{"0":"low","1":"high"},"probabilities":{"0":0.6,"1":0.4},"confidence":0.4},
  "q6":{"type":"noul","noul":0.5}
},"usage":{"input_tokens":1,"output_tokens":1}}"#;

const GHOST: &str = r#"{"model":"jev-1.13.0","answers":{
  "q0":{"type":"choice","choice":"ghost","probabilities":{"ghost":0.9,"billing":0.1},"confidence":0.9}
},"usage":{"input_tokens":1,"output_tokens":1}}"#;

async fn guide(server: &MockServer, reply: &str) -> Guide {
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_string(reply))
        .mount(server)
        .await;
    Guide::builder()
        .api_key("k".into())
        .base_url(server.uri())
        .build()
        .expect("guide")
}

#[tokio::test]
async fn a_shape_of_questions_is_one_request_with_per_question_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let guide = guide(&server, REPLY).await;

    let labels = BTreeMap::from([
        ("spam", noul("Is it spam?")),
        ("vip", noul("Is the sender a VIP?")),
    ]);
    let (urgent, dept, mood, flags): (bool, Department, Scored<Frustration>, BTreeMap<&str, bool>) =
        guide
            .ask(
                (
                    noul("Is this urgent?"),
                    choose::<Department>("Which team?")
                        .with(Policy::new().min_confidence(0.6))
                        .or(Department::Sales),
                    score::<Frustration>("How frustrated?").detail(),
                    labels,
                ),
                "Help! Payouts failing for 3 days.",
            )
            .await?;

    assert!(urgent);
    assert_eq!(dept, Department::Sales);
    assert_eq!(mood.level, Frustration::Frustrated);
    assert!((mood.value - 1.05).abs() < f64::EPSILON);
    assert_eq!(flags, BTreeMap::from([("spam", false), ("vip", true)]));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json()?;
    let questions = body["questions"].as_object().unwrap();
    assert_eq!(
        questions.keys().cloned().collect::<Vec<_>>(),
        ["q0", "q1", "q2", "q3", "q4"]
    );
    assert_eq!(questions["q1"]["criteria"]["billing"], "Payments");
    assert_eq!(body["model"], "jev-latest");
    Ok(())
}

#[tokio::test]
async fn unsure_without_fallback_fails_the_whole_call_naming_the_question() {
    let server = MockServer::start().await;
    let guide = guide(&server, REPLY)
        .await
        .with_policy(Policy::new().min_confidence(0.6))
        .unwrap();
    let err = guide
        .ask((noul("urgent?"), choose::<Department>("team?")), "x")
        .await
        .unwrap_err();
    assert!(matches!(err, guideme::Error::Unsure { ref question, .. } if question == "q1"));
}

#[tokio::test]
async fn unsure_ladder_or_beats_enum_fallback_beats_error_and_detail_never_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let guide = guide(&server, LADDER).await.with_policy(
        Policy::new()
            .yes_above(0.7)
            .no_below(0.3)
            .min_confidence(0.9),
    )?;
    let (v, a, b, c, d, e, f) = guide
        .ask(
            (
                noul("q").detail(),
                choose::<Team>("q").or(Team::Billing),
                choose::<Team>("q"),
                choose::<Team>("q").detail(),
                choose_among("q", [("billing", None::<&str>), ("sales", None)])
                    .or(Key("sales".into())),
                score_levels("q", ["low", "high"]).or(Rank(0)),
                noul("q").or(false),
            ),
            "x",
        )
        .await?;
    assert!(matches!(v, Verdict::Unsure(_)));
    assert_eq!((a, b), (Team::Billing, Team::Sales));
    assert!(c.unsure);
    assert_eq!((d, e, f), (Key("sales".into()), Rank(0), false));
    Ok(())
}

#[tokio::test]
async fn an_option_outside_the_rubric_is_a_protocol_error() {
    let server = MockServer::start().await;
    let guide = guide(&server, GHOST).await;
    let err = guide.ask(choose::<Team>("q"), "x").await.unwrap_err();
    assert!(matches!(err, guideme::Error::Protocol { .. }));
    let err = guide
        .ask(choose_among("q", [("billing", None::<&str>)]), "x")
        .await
        .unwrap_err();
    assert!(matches!(err, guideme::Error::Protocol { .. }));
}
