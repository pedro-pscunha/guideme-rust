//! Exact typed mirror of `POST /v1/systemone` and `GET /v1/models`.
//!
//! Nothing here applies policy; see [`crate::policy`]. Use [`Client`] directly when you
//! want to build the request yourself; [`crate::Guide`] does it for you.

mod client;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use client::{Client, ClientBuilder};

use crate::{Confidence, Instructions, Model, Probability, State};

/// Identifies a question inside one request. Never sent to the model.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct QuestionId(String);

impl QuestionId {
    /// The id as used in the `questions` and `answers` maps.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for QuestionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for QuestionId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::borrow::Borrow<str> for QuestionId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Optional descriptions of what a yes and a no mean for a noul question.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NoulCriteria {
    /// What a yes (value near 1) means.
    #[serde(rename = "true")]
    pub yes: String,
    /// What a no (value near 0) means.
    #[serde(rename = "false")]
    pub no: String,
}

/// One typed question. The `type` tag selects the variant on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// Yes/no; the answer is the probability of yes.
    Noul {
        /// The question.
        instructions: Instructions,
        /// Optional yes/no descriptions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    /// Pick one option; `criteria` maps option key to rubric (or `null`). Max 255.
    Choice {
        /// The question.
        instructions: Instructions,
        /// Option key → rubric.
        criteria: BTreeMap<String, Option<String>>,
    },
    /// Rate along ordered levels, low to high. 2..=10 levels.
    Score {
        /// The question.
        instructions: Instructions,
        /// Level descriptions, low to high.
        criteria: Vec<String>,
    },
}

/// The request body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    /// What to evaluate.
    pub state: State,
    /// Which model answers.
    pub model: Model,
    /// Questions keyed by ids you choose.
    pub questions: BTreeMap<QuestionId, Question>,
}

/// One typed answer. The `type` tag matches the question's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    /// Probability of yes.
    Noul {
        /// `0` is no, `1` is yes.
        noul: Probability,
    },
    /// The chosen option with the full distribution.
    Choice {
        /// Highest-probability option.
        choice: String,
        /// Every option → probability; sums to 1.
        probabilities: BTreeMap<String, Probability>,
        /// Derived from the distribution.
        confidence: Confidence,
    },
    /// Probability-weighted position on the levels.
    Score {
        /// Expected value; may land between levels.
        score: f64,
        /// Level index → description.
        #[serde(with = "level_keys")]
        #[schemars(with = "BTreeMap<String, String>")]
        legend: BTreeMap<u8, String>,
        /// Level index → probability; sums to 1.
        #[serde(with = "level_keys")]
        #[schemars(with = "BTreeMap<String, Probability>")]
        probabilities: BTreeMap<u8, Probability>,
        /// Derived from the distribution.
        confidence: Confidence,
    },
}

/// Token usage for one request. Input tokens are billed; output tokens are free.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    /// Tokens read.
    pub input_tokens: u64,
    /// Tokens written.
    pub output_tokens: u64,
}

/// The response body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    /// The versioned model that answered (e.g. `jev-1.13.0`), even when an alias was requested.
    pub model: Model,
    /// One answer per question, same ids.
    pub answers: BTreeMap<QuestionId, Answer>,
    /// Token usage.
    pub usage: Usage,
}

/// One entry from `GET /v1/models`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModelInfo {
    /// Name or alias accepted by the `model` field.
    pub name: String,
    /// What it is for.
    pub description: String,
    /// Release date as the API reports it.
    pub release_date: String,
}

/// Level indices travel as JSON object keys (`"0"`, `"1"`, …). Inside an internally tagged
/// enum serde buffers the content, so integer keys must be parsed from strings by hand.
mod level_keys {
    use std::collections::BTreeMap;

    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer, T: Serialize>(
        map: &BTreeMap<u8, T>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_map(map.iter().map(|(k, v)| (k.to_string(), v)))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        d: D,
    ) -> Result<BTreeMap<u8, T>, D::Error> {
        BTreeMap::<String, T>::deserialize(d)?
            .into_iter()
            .map(|(k, v)| {
                k.parse::<u8>()
                    .map(|k| (k, v))
                    .map_err(|e| D::Error::custom(format!("level key {k:?}: {e}")))
            })
            .collect()
    }
}

#[derive(Deserialize)]
pub(crate) struct ModelsResponse {
    pub(crate) models: Vec<ModelInfo>,
}
