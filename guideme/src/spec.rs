//! The shared contract: JSON Schemas for the wire and policy types, golden vectors for
//! [`crate::policy::resolve`], and the rubric rendering cases. `mise run spec` writes them
//! to `spec/`.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::api::{Answer, Request, Response};
use crate::policy::{Outcome, Thresholds, resolve};
use crate::{Confidence, Error, Levels, Options, Probability, Rubric};

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
    out.push(("vectors/rubric.json".to_owned(), pretty(&rubric_cases()?)?));
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

/// The choice half of the rubric vector: one variant per case, in the order of the golden
/// table in `docs/contract.md`. Split across two enums because the golden rows deliberately
/// reuse a string that no single declaration may use twice.
#[derive(Clone, PartialEq, Eq, crate::Choice)]
enum SpecChoice {
    /// Payments, invoicing, refunds
    Bare,
    /// Bugs, outages, integrations
    #[guide(example = "502 on every request")]
    OneExample,
    /// Payments, invoicing, refunds
    #[guide(example = "My card was charged twice", example = "Where is my refund?")]
    TwoExamples,
}

/// The rows that carry a counterexample, plus the case that pins declaration order: neither
/// clause is in sorted order, so a renderer that sorted would not reproduce it.
#[derive(Clone, PartialEq, Eq, crate::Choice)]
enum SpecChoiceCounter {
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

/// The score half. A level never carries a counterexample.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, crate::Levels)]
enum SpecLevel {
    /// No impact to functionality
    #[guide(example = "typo in a label", example = "misaligned icon")]
    Cosmetic,
    /// No workaround exists
    Blocking,
}

/// `(what, examples, counterexamples)` per [`SpecChoice`] variant, in declaration order.
const CHOICE_PARTS: [(&str, &[&str], &[&str]); 3] = [
    ("Payments, invoicing, refunds", &[], &[]),
    (
        "Bugs, outages, integrations",
        &["502 on every request"],
        &[],
    ),
    (
        "Payments, invoicing, refunds",
        &["My card was charged twice", "Where is my refund?"],
        &[],
    ),
];

/// The same, per [`SpecChoiceCounter`] variant.
const COUNTER_PARTS: [(&str, &[&str], &[&str]); 3] = [
    (
        "Payments, invoicing, refunds",
        &["My card was charged twice"],
        &["The dashboard is down"],
    ),
    (
        "Payments, invoicing, refunds",
        &[],
        &["The dashboard is down"],
    ),
    (
        "Payments, invoicing, refunds",
        &["Why was I billed twice?", "Cancel and refund me"],
        &["The status page is red", "Are you hiring?"],
    ),
];

/// The noul half. A noul has no enum to derive from, so these come straight from a
/// [`Rubric`], which makes them the vector's only coverage of the runtime renderer.
const NOUL_PARTS: [(&str, &[&str], &[&str]); 2] = [
    (
        "Something is broken now and nobody can work around it",
        &["the checkout page is down"],
        &["a nightly job failed and the numbers are pulled by hand for now"],
    ),
    ("It can wait for the next working day", &[], &[]),
];

/// `(what, examples)` per [`SpecLevel`] variant, low to high.
const LEVEL_PARTS: [(&str, &[&str]); 2] = [
    (
        "No impact to functionality",
        &["typo in a label", "misaligned icon"],
    ),
    ("No workaround exists", &[]),
];

/// One rubric case: what the caller wrote, and what was rendered from it. `kind` is
/// `"choice"`, `"levels"` or `"noul"` — which question kind the rubric sits in.
#[derive(Serialize)]
struct RubricCase {
    kind: &'static str,
    what: &'static str,
    examples: &'static [&'static str],
    counterexamples: &'static [&'static str],
    rendered: String,
}

/// Build the rubric vector by reading `RUBRIC` and `LEVELS` off real derived enums, so the
/// published cases are what the macro emits rather than a second copy of the algorithm.
fn rubric_cases() -> Result<Vec<RubricCase>, Error> {
    let mut out = Vec::with_capacity(
        CHOICE_PARTS.len() + COUNTER_PARTS.len() + LEVEL_PARTS.len() + NOUL_PARTS.len(),
    );
    push_choice(&mut out, SpecChoice::RUBRIC, &CHOICE_PARTS)?;
    push_choice(&mut out, SpecChoiceCounter::RUBRIC, &COUNTER_PARTS)?;
    if SpecLevel::LEVELS.len() != LEVEL_PARTS.len() {
        return Err(Error::Config {
            detail: "the level grid and its enum disagree on length".into(),
        });
    }
    for (rendered, (what, examples)) in SpecLevel::LEVELS.iter().zip(LEVEL_PARTS) {
        out.push(rubric_case("levels", what, examples, &[], rendered)?);
    }
    for (what, examples, counterexamples) in NOUL_PARTS {
        let rendered = rubric(what, examples, counterexamples).render()?;
        out.push(RubricCase {
            kind: "noul",
            what,
            examples,
            counterexamples,
            rendered,
        });
    }
    Ok(out)
}

/// Build the runtime rubric a case describes.
fn rubric(what: &str, examples: &[&str], counterexamples: &[&str]) -> Rubric {
    let mut rubric = Rubric::new(what);
    for example in examples {
        rubric = rubric.example(*example);
    }
    for counterexample in counterexamples {
        rubric = rubric.counterexample(*counterexample);
    }
    rubric
}

/// Append one derived choice enum's cases, pairing each variant with the parts it was written
/// from.
fn push_choice(
    out: &mut Vec<RubricCase>,
    rubric: &'static [(&'static str, Option<&'static str>)],
    parts: &'static [(
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    )],
) -> Result<(), Error> {
    if rubric.len() != parts.len() {
        return Err(Error::Config {
            detail: "the rubric grid and its enum disagree on length".into(),
        });
    }
    for ((_, rendered), (what, examples, counterexamples)) in
        rubric.iter().zip(parts.iter().copied())
    {
        let rendered = rendered.ok_or_else(|| Error::Config {
            detail: format!("rubric case {what:?} lost its rubric"),
        })?;
        out.push(rubric_case(
            "choice",
            what,
            examples,
            counterexamples,
            rendered,
        )?);
    }
    Ok(())
}

/// Pair one case with what the derive rendered, rejecting a grid that has drifted from its
/// enum — and, because the runtime renderer is a second copy of the algorithm, refusing to
/// publish a vector the two do not agree on.
fn rubric_case(
    kind: &'static str,
    what: &'static str,
    examples: &'static [&'static str],
    counterexamples: &'static [&'static str],
    rendered: &'static str,
) -> Result<RubricCase, Error> {
    let from_rubric = rubric(what, examples, counterexamples).render()?;
    if from_rubric != rendered {
        return Err(Error::Config {
            detail: format!(
                "rubric case {what:?}: the derive rendered {rendered:?}, Rubric rendered {from_rubric:?}"
            ),
        });
    }
    Ok(RubricCase {
        kind,
        what,
        examples,
        counterexamples,
        rendered: from_rubric,
    })
}
