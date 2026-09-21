//! The pure decision layer: thresholds in, labelled outcome out. No I/O, no generics.
//! This is the function other-language SDKs port; `spec/vectors/policy.json` is its contract.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::{Answer, MAX_LEVELS};
use crate::{Confidence, Error, Probability};

/// A policy patch. Unset fields defer to the next layer (guide, then crate defaults).
///
/// Build with [`Policy::new`] and the `const fn` setters; house policies can be constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub struct Policy {
    /// Noul: `p >= yes_above` is yes.
    pub yes_above: Option<f64>,
    /// Noul: `p <= no_below` is no.
    pub no_below: Option<f64>,
    /// Choice/score: `confidence < min_confidence` is unsure.
    pub min_confidence: Option<f64>,
}

impl Policy {
    /// An empty patch.
    pub const fn new() -> Self {
        Self {
            yes_above: None,
            no_below: None,
            min_confidence: None,
        }
    }
    /// Set `yes_above`.
    pub const fn yes_above(mut self, p: f64) -> Self {
        self.yes_above = Some(p);
        self
    }
    /// Set `no_below`.
    pub const fn no_below(mut self, p: f64) -> Self {
        self.no_below = Some(p);
        self
    }
    /// Set `min_confidence`.
    pub const fn min_confidence(mut self, c: f64) -> Self {
        self.min_confidence = Some(c);
        self
    }
    /// `self` wins over `base`, field by field.
    pub fn over(self, base: Policy) -> Policy {
        Policy {
            yes_above: self.yes_above.or(base.yes_above),
            no_below: self.no_below.or(base.no_below),
            min_confidence: self.min_confidence.or(base.min_confidence),
        }
    }
    /// Fill unset fields with the crate defaults and validate.
    pub fn settle(self) -> Result<Thresholds, Error> {
        let d = Thresholds::default();
        Thresholds::new(
            self.yes_above.unwrap_or(d.yes_above),
            self.no_below.unwrap_or(d.no_below),
            self.min_confidence.unwrap_or(d.min_confidence),
        )
    }
}

/// Fully settled, validated thresholds: the input of `resolve` and of every golden vector.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ThresholdsRepr")]
pub struct Thresholds {
    yes_above: f64,
    no_below: f64,
    min_confidence: f64,
}

/// Wire shape of [`Thresholds`]; validated by `TryFrom` on the way in.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "Thresholds")]
struct ThresholdsRepr {
    /// Noul yes boundary (inclusive).
    #[schemars(range(min = 0.0, max = 1.0))]
    yes_above: f64,
    /// Noul no boundary (inclusive); must not exceed `yes_above`.
    #[schemars(range(min = 0.0, max = 1.0))]
    no_below: f64,
    /// Choice/score confidence floor.
    #[schemars(range(min = 0.0, max = 1.0))]
    min_confidence: f64,
}

impl TryFrom<ThresholdsRepr> for Thresholds {
    type Error = Error;
    fn try_from(r: ThresholdsRepr) -> Result<Self, Error> {
        Self::new(r.yes_above, r.no_below, r.min_confidence)
    }
}

impl Thresholds {
    /// Validate: every field in `0..=1`, `no_below <= yes_above`.
    pub fn new(yes_above: f64, no_below: f64, min_confidence: f64) -> Result<Self, Error> {
        for (name, v) in [
            ("yes_above", yes_above),
            ("no_below", no_below),
            ("min_confidence", min_confidence),
        ] {
            if !(0.0..=1.0).contains(&v) {
                return Err(Error::Config {
                    detail: format!("{name} = {v} is outside 0..=1"),
                });
            }
        }
        if no_below > yes_above {
            return Err(Error::Config {
                detail: format!("no_below {no_below} > yes_above {yes_above}"),
            });
        }
        Ok(Self {
            yes_above,
            no_below,
            min_confidence,
        })
    }
    /// Noul yes boundary (inclusive).
    pub const fn yes_above(self) -> f64 {
        self.yes_above
    }
    /// Noul no boundary (inclusive).
    pub const fn no_below(self) -> f64 {
        self.no_below
    }
    /// Choice/score confidence floor.
    pub const fn min_confidence(self) -> f64 {
        self.min_confidence
    }
}

impl Default for Thresholds {
    /// `0.5 / 0.5 / 0.0`: no unsure band, never unsure on confidence.
    fn default() -> Self {
        Self {
            yes_above: 0.5,
            no_below: 0.5,
            min_confidence: 0.0,
        }
    }
}

/// The three-way reading of a noul answer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "verdict", content = "p", rename_all = "lowercase")]
pub enum Verdict {
    /// `p >= yes_above`.
    Yes(Probability),
    /// `p <= no_below`.
    No(Probability),
    /// Strictly between the two.
    Unsure(Probability),
}

/// What the policy concludes about one answer. Untyped: keys and indices, not Rust enums.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Outcome {
    /// Noul.
    Noul(Verdict),
    /// Choice.
    Choice {
        /// The API's chosen key.
        key: String,
        /// Reported confidence.
        confidence: Confidence,
        /// `confidence < min_confidence`.
        unsure: bool,
        /// Options by descending probability, ties by key.
        ranked: Vec<(String, Probability)>,
    },
    /// Score. Levels are positional: index `i` of `distribution` and `legend` is level `i`.
    Score {
        /// Argmax of the distribution; ties go to the lowest index.
        index: usize,
        /// The API's expected value.
        value: f64,
        /// Reported confidence.
        confidence: Confidence,
        /// `confidence < min_confidence`.
        unsure: bool,
        /// Probability of each level, in level order.
        distribution: Vec<Probability>,
        /// Description of each level, in level order.
        legend: Vec<String>,
    },
}

/// Apply thresholds to one answer. Pure and total except for malformed answers.
///
/// # Errors
/// [`Error::Protocol`] when the chosen key is absent from the distribution, the legend and
/// distribution are not the same contiguous `0..n` with `2 <= n <= 10`, or the score lies
/// outside the level range.
pub fn resolve(answer: &Answer, t: Thresholds) -> Result<Outcome, Error> {
    match answer {
        Answer::Noul { noul } => {
            let p = noul.get();
            Ok(Outcome::Noul(if p >= t.yes_above {
                Verdict::Yes(*noul)
            } else if p <= t.no_below {
                Verdict::No(*noul)
            } else {
                Verdict::Unsure(*noul)
            }))
        }
        Answer::Choice {
            choice,
            probabilities,
            confidence,
        } => {
            if !probabilities.contains_key(choice) {
                return Err(Error::Protocol {
                    detail: format!("choice {choice:?} is not in the distribution"),
                });
            }
            let mut ranked: Vec<(String, Probability)> =
                probabilities.iter().map(|(k, p)| (k.clone(), *p)).collect();
            ranked.sort_by(|a, b| b.1.get().total_cmp(&a.1.get()).then_with(|| a.0.cmp(&b.0)));
            Ok(Outcome::Choice {
                key: choice.clone(),
                confidence: *confidence,
                unsure: confidence.get() < t.min_confidence,
                ranked,
            })
        }
        Answer::Score {
            score,
            legend,
            probabilities,
            confidence,
        } => {
            let n = legend.len();
            let contiguous = legend.keys().enumerate().all(|(i, k)| usize::from(*k) == i)
                && probabilities
                    .keys()
                    .enumerate()
                    .all(|(i, k)| usize::from(*k) == i)
                && probabilities.len() == n;
            if !(2..=MAX_LEVELS).contains(&n) || !contiguous {
                return Err(Error::Protocol {
                    detail: format!(
                        "score legend/probabilities must be contiguous levels 0..n with 2 <= n <= {MAX_LEVELS}, got {n}"
                    ),
                });
            }
            let Ok(last) = u8::try_from(n - 1) else {
                return Err(Error::Protocol {
                    detail: format!("score has {n} levels"),
                });
            };
            let max = f64::from(last);
            if !(0.0..=max).contains(score) {
                return Err(Error::Protocol {
                    detail: format!("score {score} is outside 0..={max}"),
                });
            }
            let index = probabilities.iter().fold(0u8, |best, (k, p)| {
                let best_p = probabilities.get(&best).map_or(0.0, |b| b.get());
                if p.get() > best_p { *k } else { best }
            });
            Ok(Outcome::Score {
                index: usize::from(index),
                value: *score,
                confidence: *confidence,
                unsure: confidence.get() < t.min_confidence,
                distribution: probabilities.values().copied().collect(),
                legend: legend.values().cloned().collect(),
            })
        }
    }
}
