//! The porting contract: JSON Schemas for the wire and policy types, and golden vectors
//! for [`crate::policy::resolve`]. `mise run spec` writes them to `spec/`.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::api::{Answer, Request, Response};
use crate::policy::{Outcome, Thresholds, resolve};
use crate::{Confidence, Error, Probability};

/// How many vectors [`render`] produces. Guarded so the contract cannot silently shrink.
pub const EXPECTED_VECTORS: usize = 42;

/// One golden case: either `outcome` or `error` is present.
#[derive(Serialize)]
struct Vector {
    answer: Answer,
    thresholds: Thresholds,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}

const NOULS: [f64; 7] = [0.0, 0.25, 0.3, 0.5, 0.7, 0.75, 1.0];
const CHOICES: [([f64; 3], f64); 3] = [
    ([0.88, 0.12, 0.0], 0.81),
    ([0.4, 0.35, 0.25], 0.2),
    ([1.0, 0.0, 0.0], 1.0),
];
const SCORES: [([f64; 3], f64, f64); 3] = [
    ([0.0, 0.95, 0.05], 1.05, 0.92),
    ([0.57, 0.43, 0.0], 0.43, 0.35),
    ([0.2, 0.3, 0.5], 1.3, 0.5),
];
const THRESHOLDS: [(f64, f64, f64); 3] = [(0.5, 0.5, 0.0), (0.7, 0.3, 0.0), (0.5, 0.5, 0.8)];

/// Render every spec file as `(relative path, contents)`. Deterministic.
///
/// # Errors
/// [`Error::Config`] if the grid is malformed or the vector count drifts from
/// [`EXPECTED_VECTORS`]; [`Error::Protocol`] if a schema or vector cannot be serialised.
pub fn render() -> Result<Vec<(String, String)>, Error> {
    let mut out = vec![
        (
            "schema/request.json".to_owned(),
            pretty(&schemars::schema_for!(Request))?,
        ),
        (
            "schema/response.json".to_owned(),
            pretty(&schemars::schema_for!(Response))?,
        ),
        (
            "schema/thresholds.json".to_owned(),
            pretty(&schemars::schema_for!(Thresholds))?,
        ),
        (
            "schema/outcome.json".to_owned(),
            pretty(&schemars::schema_for!(Outcome))?,
        ),
    ];
    let vectors = vectors()?;
    if vectors.len() != EXPECTED_VECTORS {
        return Err(Error::Config {
            detail: format!(
                "spec grid produced {} vectors, expected {EXPECTED_VECTORS}",
                vectors.len()
            ),
        });
    }
    out.push(("vectors/policy.json".to_owned(), pretty(&vectors)?));
    Ok(out)
}

fn pretty<T: Serialize>(v: &T) -> Result<String, Error> {
    let mut s = serde_json::to_string_pretty(v).map_err(|e| Error::Protocol {
        detail: format!("spec serialisation: {e}"),
    })?;
    s.push('\n');
    Ok(s)
}

fn probs3(p: [f64; 3]) -> Result<[Probability; 3], Error> {
    Ok([
        Probability::try_from(p[0])?,
        Probability::try_from(p[1])?,
        Probability::try_from(p[2])?,
    ])
}

fn choice(keys: [&str; 3], p: [f64; 3], confidence: f64) -> Result<Answer, Error> {
    let p = probs3(p)?;
    Ok(Answer::Choice {
        choice: keys[0].to_owned(),
        probabilities: keys
            .iter()
            .zip(p)
            .map(|(k, p)| ((*k).to_owned(), p))
            .collect(),
        confidence: Confidence::try_from(confidence)?,
    })
}

fn score(p: [f64; 3], value: f64, confidence: f64) -> Result<Answer, Error> {
    let p = probs3(p)?;
    Ok(Answer::Score {
        score: value,
        legend: BTreeMap::from([
            (0, "Calm".to_owned()),
            (1, "Frustrated".to_owned()),
            (2, "Very angry".to_owned()),
        ]),
        probabilities: (0u8..3).zip(p).collect(),
        confidence: Confidence::try_from(confidence)?,
    })
}

fn vectors() -> Result<Vec<Vector>, Error> {
    let mut answers = Vec::new();
    for v in NOULS {
        answers.push(Answer::Noul {
            noul: Probability::try_from(v)?,
        });
    }
    for (p, c) in CHOICES {
        answers.push(choice(["billing", "technical", "sales"], p, c)?);
    }
    for (p, v, c) in SCORES {
        answers.push(score(p, v, c)?);
    }
    let mut out = Vec::new();
    for answer in answers {
        for (y, n, m) in THRESHOLDS {
            let thresholds = Thresholds::new(y, n, m)?;
            out.push(Vector {
                answer: answer.clone(),
                thresholds,
                outcome: Some(resolve(&answer, thresholds)?),
                error: None,
            });
        }
    }
    let one = Probability::try_from(1.0)?;
    let zero = Probability::try_from(0.0)?;
    let full = Confidence::try_from(1.0)?;
    let rejected = [
        Answer::Choice {
            choice: "ghost".to_owned(),
            probabilities: BTreeMap::from([("billing".to_owned(), one)]),
            confidence: full,
        },
        Answer::Score {
            score: 0.0,
            legend: BTreeMap::from([(0, "a".to_owned()), (2, "c".to_owned())]),
            probabilities: BTreeMap::from([(0, one), (2, zero)]),
            confidence: full,
        },
        Answer::Score {
            score: 0.0,
            legend: (0u8..11).map(|i| (i, format!("L{i}"))).collect(),
            probabilities: (0u8..11)
                .map(|i| (i, if i == 0 { one } else { zero }))
                .collect(),
            confidence: full,
        },
    ];
    for answer in rejected {
        let thresholds = Thresholds::default();
        if resolve(&answer, thresholds).is_ok() {
            return Err(Error::Config {
                detail: "a negative vector unexpectedly resolved".into(),
            });
        }
        out.push(Vector {
            answer,
            thresholds,
            outcome: None,
            error: Some("protocol"),
        });
    }
    Ok(out)
}
