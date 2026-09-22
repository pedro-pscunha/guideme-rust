#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use guideme::api::{Answer, Client, Question, Request, Response, Usage};
use guideme::{ApiKey, Error, Instructions, Model, State};
use proptest::prelude::*;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NOUL: &str = r#"{"model":"jev-1.13.0","answers":{"is_urgent":{"type":"noul","noul":0.95}},"usage":{"input_tokens":307,"output_tokens":20}}"#;
const CHOICE: &str = r#"{"model":"jev-1.13.0","answers":{"department":{"type":"choice","choice":"billing","probabilities":{"billing":0.88,"technical":0.12,"sales":0.0},"confidence":0.81}},"usage":{"input_tokens":318,"output_tokens":34}}"#;
const SCORE: &str = r#"{"model":"jev-1.13.0","answers":{"frustration":{"type":"score","score":1.05,"legend":{"0":"Calm","1":"Frustrated","2":"Very angry"},"probabilities":{"0":0.0,"1":0.95,"2":0.05},"confidence":0.92}},"usage":{"input_tokens":304,"output_tokens":18}}"#;

#[test]
fn examples_from_docs_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    for text in [NOUL, CHOICE, SCORE] {
        let parsed: Response = serde_json::from_str(text)?;
        let back = serde_json::to_value(&parsed)?;
        let original: serde_json::Value = serde_json::from_str(text)?;
        assert_eq!(back, original);
    }
    let choice: Response = serde_json::from_str(CHOICE)?;
    match &choice.answers["department"] {
        Answer::Choice {
            choice, confidence, ..
        } => {
            assert_eq!(choice, "billing");
            assert!((confidence.get() - 0.81).abs() < f64::EPSILON);
        }
        Answer::Noul { .. } | Answer::Score { .. } => panic!("wrong kind"),
    }
    Ok(())
}

fn prob() -> impl Strategy<Value = f64> {
    (0u32..=1000).prop_map(|n| f64::from(n) / 1000.0)
}

fn question() -> impl Strategy<Value = Question> {
    prop_oneof![
        "[a-z ?]{1,40}".prop_map(|q| Question::Noul {
            instructions: Instructions::from(q),
            criteria: None
        }),
        (
            "[a-z ?]{1,40}",
            prop::collection::btree_map("[a-z_]{1,8}", prop::option::of("[a-z ]{1,20}"), 1..6)
        )
            .prop_map(|(q, c)| Question::Choice {
                instructions: Instructions::from(q),
                criteria: c
            }),
        (
            "[a-z ?]{1,40}",
            prop::collection::vec("[a-z ]{1,20}", 2..=10)
        )
            .prop_map(|(q, c)| Question::Score {
                instructions: Instructions::from(q),
                criteria: c
            }),
    ]
}

fn answer() -> impl Strategy<Value = Answer> {
    prop_oneof![
        prob().prop_map(|p| Answer::Noul {
            noul: p.try_into().unwrap()
        }),
        (
            prop::collection::btree_map("[a-z_]{1,8}", prob(), 1..6),
            prob()
        )
            .prop_map(|(m, c)| {
                let choice = m
                    .iter()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(k, _)| k.clone())
                    .unwrap();
                Answer::Choice {
                    choice,
                    probabilities: m
                        .into_iter()
                        .map(|(k, p)| (k, p.try_into().unwrap()))
                        .collect(),
                    confidence: c.try_into().unwrap(),
                }
            }),
        (prop::collection::vec(prob(), 2..=10), prob()).prop_map(|(ps, c)| {
            let n = ps.len() as u8;
            Answer::Score {
                score: f64::from(n - 1) / 2.0,
                legend: (0..n).map(|i| (i, format!("level {i}"))).collect(),
                probabilities: ps
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| (i as u8, p.try_into().unwrap()))
                    .collect(),
                confidence: c.try_into().unwrap(),
            }
        }),
    ]
}

proptest! {
    #[test]
    fn api_request_and_response_survive_serde_round_trip(
        qs in prop::collection::btree_map("q[0-9]{1,2}", question(), 1..4),
        ans in prop::collection::btree_map("q[0-9]{1,2}", answer(), 1..4),
    ) {
        let req = Request {
            state: State::from("hello"),
            model: Model::latest(),
            questions: qs.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        };
        let text = serde_json::to_string(&req)?;
        let back: Request = serde_json::from_str(&text)?;
        prop_assert_eq!(back, req);

        let res = Response {
            model: Model::new("jev-1.13.0"),
            answers: ans.into_iter().map(|(k, v)| (k.into(), v)).collect::<BTreeMap<_, _>>(),
            usage: Usage { input_tokens: 1, output_tokens: 2 },
        };
        let text = serde_json::to_string(&res)?;
        let back: Response = serde_json::from_str(&text)?;
        prop_assert_eq!(back, res);
    }
}

fn client(server: &MockServer, retries: u32) -> Client {
    Client::builder(ApiKey::from("test-key"))
        .base_url(server.uri())
        .max_retries(retries)
        .backoff(Duration::from_millis(10))
        .build()
        .expect("client")
}

fn request() -> Request {
    Request {
        state: State::from("Help! My payouts have been failing for 3 days."),
        model: Model::latest(),
        questions: [(
            "is_urgent".into(),
            Question::Noul {
                instructions: "Does this convey urgency?".into(),
                criteria: None,
            },
        )]
        .into(),
    }
}

#[tokio::test]
async fn rate_limit_is_retried_after_the_advertised_delay() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NOUL))
        .mount(&server)
        .await;

    let started = Instant::now();
    let response = client(&server, 3).evaluate(&request()).await?;
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    assert!(matches!(response.answers["is_urgent"], Answer::Noul { .. }));
    Ok(())
}

#[tokio::test]
async fn unauthorized_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = client(&server, 3).evaluate(&request()).await.unwrap_err();
    assert!(matches!(err, Error::Auth));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn overload_exhausts_retries_then_fails() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529))
        .mount(&server)
        .await;
    let err = client(&server, 2).evaluate(&request()).await.unwrap_err();
    assert!(matches!(err, Error::Overloaded { retry_after: None }));
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    // A `retry-after` longer than the cap is not waited for on a 529 either, and the duration
    // the API asked for comes back on the error instead of being dropped.
    let patient = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529).insert_header("retry-after", "60"))
        .mount(&patient)
        .await;
    let err = client(&patient, 2).evaluate(&request()).await.unwrap_err();
    assert!(
        matches!(err, Error::Overloaded { retry_after: Some(d) } if d == Duration::from_secs(60)),
        "the 529 should carry the header it parsed, got {err:?}"
    );
    assert_eq!(patient.received_requests().await.unwrap().len(), 1);
}

const MODELS: &str = r#"{"models":[{"name":"jev-latest","description":"The most recent stable release","release_date":"2026-08-01"}]}"#;

/// The docs say the SDKs handle a throttle automatically, and they say it about the API, not
/// about one endpoint: a `429` on a startup `models()` call used to fail the boot.
#[tokio::test]
async fn listing_models_is_retried_on_a_throttle() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(529))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
        .mount(&server)
        .await;

    let models = client(&server, 3).models().await?;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "jev-latest");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_body_that_violates_the_contract_is_a_protocol_error() {
    let server = MockServer::start().await;
    let bad = r#"{"model":"jev-1.13.0","answers":{"is_urgent":{"type":"noul","noul":1.5}},"usage":{"input_tokens":1,"output_tokens":1}}"#;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bad))
        .mount(&server)
        .await;
    let err = client(&server, 0).evaluate(&request()).await.unwrap_err();
    assert!(matches!(err, Error::Protocol { .. }));
}
