//! The product: one verb, `ask`, over any question shape.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{Instrument, field, info_span};

use crate::api::{Client, ModelInfo, QuestionId, Request};
use crate::ask::{Ask, Plan, Reply};
use crate::policy::{self, Outcome, Thresholds, Verdict};
use crate::{ApiKey, Error, Model, Policy, State};

/// A configured entry point to Jev. Cheap to clone; share it.
#[derive(Clone, Debug)]
pub struct Guide {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    client: Client,
    model: Model,
    policy: Policy,
    record_state: bool,
}

/// Configures a [`Guide`].
#[derive(Debug)]
pub struct GuideBuilder {
    api_key: Option<ApiKey>,
    base_url: Option<String>,
    model: Model,
    policy: Policy,
    max_retries: u32,
    timeout: Duration,
    record_state: bool,
}

impl Guide {
    /// Read `TYPESAFE_API_KEY` (required), `TYPESAFE_BASE_URL` and `GUIDEME_MODEL` (optional).
    pub fn from_env() -> Result<Self, Error> {
        let key = std::env::var("TYPESAFE_API_KEY").map_err(|_| Error::Config {
            detail: "TYPESAFE_API_KEY is not set".into(),
        })?;
        let mut b = Self::builder().api_key(ApiKey::from(key));
        if let Ok(url) = std::env::var("TYPESAFE_BASE_URL") {
            b = b.base_url(url);
        }
        if let Ok(model) = std::env::var("GUIDEME_MODEL") {
            b = b.model(Model::new(model));
        }
        b.build()
    }

    /// Start configuring a guide.
    pub fn builder() -> GuideBuilder {
        GuideBuilder {
            api_key: None,
            base_url: None,
            model: Model::latest(),
            policy: Policy::new(),
            max_retries: 3,
            timeout: Duration::from_secs(30),
            record_state: false,
        }
    }

    /// A guide sharing this client with `policy` patched over this guide's policy.
    pub fn with_policy(&self, policy: Policy) -> Guide {
        Guide {
            inner: Arc::new(Inner {
                client: self.inner.client.clone(),
                model: self.inner.model.clone(),
                policy: policy.over(self.inner.policy),
                record_state: self.inner.record_state,
            }),
        }
    }

    /// Answer `ask` about `state` in exactly one request and one `guideme.ask` span.
    ///
    /// `ask` is a [`crate::Question`], a tuple of shapes (up to 8), a `Vec` of shapes, or a
    /// `BTreeMap` of shapes; the output has the same shape. Question ids are `q0..qN` in
    /// encounter order. The call is atomic: one failing answer fails the whole call.
    pub async fn ask<A: Ask>(&self, ask: A, state: impl Into<State>) -> Result<A::Out, Error> {
        let state: State = state.into();
        let state_json = serde_json::to_string(state.value()?).map_err(|e| Error::Config {
            detail: e.to_string(),
        })?;
        let mut plan = Plan::new(self.inner.policy);
        let claim = ask.encode(&mut plan)?;
        if plan.questions.is_empty() {
            return Err(Error::Config {
                detail: "a batch needs at least one question".into(),
            });
        }
        let span = info_span!(
            "guideme.ask",
            model.requested = self.inner.model.as_str(),
            model.answered = field::Empty,
            questions = plan.questions.len(),
            state.bytes = state_json.len(),
            state = field::Empty,
            usage.input_tokens = field::Empty,
            usage.output_tokens = field::Empty,
            retries = field::Empty,
            elapsed_ms = field::Empty,
        );
        if self.inner.record_state {
            span.record("state", state_json.as_str());
        }
        let started = Instant::now();
        let request = Request {
            state,
            model: self.inner.model.clone(),
            questions: plan.questions,
        };
        let (response, retries) = self
            .inner
            .client
            .evaluate_counted(&request)
            .instrument(span.clone())
            .await?;
        span.record("model.answered", response.model.as_str());
        span.record("usage.input_tokens", response.usage.input_tokens);
        span.record("usage.output_tokens", response.usage.output_tokens);
        span.record("retries", retries);
        span.record(
            "elapsed_ms",
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        );

        let _entered = span.enter();
        let mut outcomes = BTreeMap::new();
        for (id, t) in plan.thresholds {
            let answer = response.answers.get(&id).ok_or_else(|| Error::Protocol {
                detail: format!("no answer for question {}", id.as_str()),
            })?;
            let outcome = policy::resolve(answer, t)?;
            emit(&id, &outcome, t);
            outcomes.insert(id, (outcome, t));
        }
        A::decode(claim, &Reply { outcomes })
    }

    /// Models the account may use.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, Error> {
        self.inner.client.models().await
    }
}

fn emit(id: &QuestionId, outcome: &Outcome, t: Thresholds) {
    match outcome {
        Outcome::Noul(v) => {
            let (label, p) = match v {
                Verdict::Yes(p) => ("yes", p),
                Verdict::No(p) => ("no", p),
                Verdict::Unsure(p) => ("unsure", p),
            };
            tracing::info!(
                name: "guideme.answer",
                question = id.as_str(),
                kind = "noul",
                outcome = label,
                probability = p.get(),
                unsure = matches!(v, Verdict::Unsure(_)),
                yes_above = t.yes_above(),
                no_below = t.no_below(),
                min_confidence = t.min_confidence(),
            );
        }
        Outcome::Choice {
            key,
            confidence,
            unsure,
            ..
        } => {
            tracing::info!(
                name: "guideme.answer",
                question = id.as_str(),
                kind = "choice",
                outcome = key.as_str(),
                confidence = confidence.get(),
                unsure = *unsure,
                yes_above = t.yes_above(),
                no_below = t.no_below(),
                min_confidence = t.min_confidence(),
            );
        }
        Outcome::Score {
            index,
            value,
            confidence,
            unsure,
            ..
        } => {
            tracing::info!(
                name: "guideme.answer",
                question = id.as_str(),
                kind = "score",
                outcome = *index,
                value = *value,
                confidence = confidence.get(),
                unsure = *unsure,
                yes_above = t.yes_above(),
                no_below = t.no_below(),
                min_confidence = t.min_confidence(),
            );
        }
    }
}

impl GuideBuilder {
    /// The API key. Required unless built by [`Guide::from_env`].
    pub fn api_key(mut self, key: ApiKey) -> Self {
        self.api_key = Some(key);
        self
    }
    /// Override the API origin.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }
    /// The model or alias; default `jev-latest`.
    pub fn model(mut self, model: Model) -> Self {
        self.model = model;
        self
    }
    /// The guide-wide policy patch.
    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }
    /// Retries for `429`/`529`; default 3.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }
    /// Per-request timeout; default 30 s.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }
    /// Record the state JSON on the span. Off by default: state is user data.
    pub fn record_state(mut self, on: bool) -> Self {
        self.record_state = on;
        self
    }
    /// Build. Validates the policy now so a bad house policy fails at startup.
    pub fn build(self) -> Result<Guide, Error> {
        let key = self.api_key.ok_or_else(|| Error::Config {
            detail: "api_key is required".into(),
        })?;
        self.policy.settle()?;
        let mut client = Client::builder(key)
            .max_retries(self.max_retries)
            .timeout(self.timeout);
        if let Some(url) = self.base_url {
            client = client.base_url(url);
        }
        Ok(Guide {
            inner: Arc::new(Inner {
                client: client.build()?,
                model: self.model,
                policy: self.policy,
                record_state: self.record_state,
            }),
        })
    }
}
