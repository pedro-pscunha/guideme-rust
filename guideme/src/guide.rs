//! The product: one verb, `ask`, over any question shape.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tracing::{Instrument, field, info_span};

use crate::api::{Client, ModelInfo, QuestionId, Request};
use crate::ask::{Ask, Plan, Reply};
use crate::policy::{self, Outcome, Thresholds, Verdict};
use crate::{ApiKey, Error, Model, Policy, State};

const TARGET: &str = "guideme";

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
    /// Validated now, like [`GuideBuilder::build`], so a bad patch fails where it is written.
    pub fn with_policy(&self, policy: Policy) -> Result<Guide, Error> {
        let policy = policy.over(self.inner.policy);
        policy.settle()?;
        Ok(Guide {
            inner: Arc::new(Inner {
                client: self.inner.client.clone(),
                model: self.inner.model.clone(),
                policy,
                record_state: self.inner.record_state,
            }),
        })
    }

    /// Answer `ask` about `state` in exactly one request and one `guideme.ask` span.
    ///
    /// `ask` is a [`crate::Question`], a tuple of shapes (up to 8), a `Vec` of shapes, or a
    /// `BTreeMap` of shapes; the output has the same shape. Question ids are `q0..qN` in
    /// encounter order. The call is atomic: one failing answer fails the whole call.
    pub async fn ask<A: Ask>(&self, ask: A, state: impl Into<State>) -> Result<A::Out, Error> {
        let state: State = state.into();
        // The state is serialised here for `guideme.state.bytes` and again by the client; fold
        // into one pass if document-sized states show up in profiles.
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
        let (host, port) = self.inner.client.server();
        // Field names follow the OpenTelemetry GenAI and HTTP semantic conventions where one
        // exists and are namespaced under `guideme.` otherwise. The `otel.*` fields are read by
        // `tracing-opentelemetry` to set the exported span's kind and status; any other
        // subscriber sees them as ordinary fields.
        let span = info_span!(
            target: TARGET,
            "guideme.ask",
            otel.kind = "client",
            gen_ai.provider.name = "typesafe",
            gen_ai.operation.name = "ask",
            gen_ai.request.model = self.inner.model.as_str(),
            gen_ai.response.model = field::Empty,
            gen_ai.usage.input_tokens = field::Empty,
            gen_ai.usage.output_tokens = field::Empty,
            server.address = host,
            server.port = i64::from(port),
            guideme.questions = signed(plan.questions.len()),
            guideme.state.bytes = signed(state_json.len()),
            guideme.state = field::Empty,
            error.type = field::Empty,
            otel.status_code = field::Empty,
            otel.status_description = field::Empty,
        );
        if self.inner.record_state {
            span.record("guideme.state", state_json.as_str());
        }
        let request = Request {
            state,
            model: self.inner.model.clone(),
            questions: plan.questions,
        };
        let result = self
            .inner
            .client
            .evaluate(&request)
            .instrument(span.clone())
            .await;
        let response = match result {
            Ok(response) => response,
            Err(e) => {
                fail(&span, &e);
                return Err(e);
            }
        };
        span.record("gen_ai.response.model", response.model.as_str());
        span.record(
            "gen_ai.usage.input_tokens",
            signed(response.usage.input_tokens),
        );
        span.record(
            "gen_ai.usage.output_tokens",
            signed(response.usage.output_tokens),
        );

        let decoded = span.in_scope(|| {
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
        });
        if let Err(e) = &decoded {
            fail(&span, e);
        }
        decoded
    }

    /// Models the account may use.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, Error> {
        self.inner.client.models().await
    }
}

/// Marks the ask span as failed: a stable `error.type`, and the status OpenTelemetry expects.
/// The description is recorded last because `tracing-opentelemetry` lets the last status
/// field written win.
fn fail(span: &tracing::Span, error: &Error) {
    span.record("error.type", error.kind());
    span.record("otel.status_code", "ERROR");
    span.record("otel.status_description", describe(error).as_str());
}

/// The status description. Two variants carry a verbatim response body, which could echo
/// the state; those stay on the returned error and off the span.
fn describe(error: &Error) -> String {
    match error {
        Error::Invalid { .. } => "invalid request: the 422 body is on the returned error".into(),
        Error::UnexpectedStatus { status, .. } => {
            format!("unexpected status {status}: the body is on the returned error")
        }
        Error::Auth
        | Error::RateLimited { .. }
        | Error::Overloaded
        | Error::Transport(_)
        | Error::Protocol { .. }
        | Error::Unsure { .. }
        | Error::Config { .. } => error.to_string(),
    }
}

/// OpenTelemetry attributes are signed, and `tracing-opentelemetry` has no `u64` path: an
/// unsigned field is exported as a string. Clamping into `i64` keeps these numbers numeric
/// for whatever consumes the trace. The clamp only bites above 2^63, which no count here
/// reaches: lengths are bounded by memory and token counts by the request size.
fn signed(n: impl TryInto<i64>) -> i64 {
    n.try_into().unwrap_or(i64::MAX)
}

fn emit(id: &QuestionId, outcome: &Outcome, t: Thresholds) {
    match outcome {
        Outcome::Noul(v) => {
            let (label, p) = match v {
                Verdict::Yes(p) => ("yes", p),
                Verdict::No(p) => ("no", p),
                Verdict::Unsure(p) => ("unsure", p),
            };
            tracing::event!(
                name: "guideme.answer",
                target: TARGET,
                tracing::Level::INFO,
                guideme.question = id.as_str(),
                guideme.kind = "noul",
                guideme.outcome = label,
                guideme.probability = p.get(),
                guideme.unsure = matches!(v, Verdict::Unsure(_)),
                guideme.yes_above = t.yes_above(),
                guideme.no_below = t.no_below(),
                guideme.min_confidence = t.min_confidence(),
            "{} noul: {label}",
            id.as_str(),
            );
        }
        Outcome::Choice {
            key,
            confidence,
            unsure,
            ..
        } => {
            tracing::event!(
                name: "guideme.answer",
                target: TARGET,
                tracing::Level::INFO,
                guideme.question = id.as_str(),
                guideme.kind = "choice",
                guideme.outcome = key.as_str(),
                guideme.confidence = confidence.get(),
                guideme.unsure = *unsure,
                guideme.yes_above = t.yes_above(),
                guideme.no_below = t.no_below(),
                guideme.min_confidence = t.min_confidence(),
            "{} choice: {}",
            id.as_str(),
            key.as_str(),
            );
        }
        Outcome::Score {
            index,
            value,
            confidence,
            unsure,
            ..
        } => {
            tracing::event!(
                name: "guideme.answer",
                target: TARGET,
                tracing::Level::INFO,
                guideme.question = id.as_str(),
                guideme.kind = "score",
                guideme.outcome = signed(*index),
                guideme.value = *value,
                guideme.confidence = confidence.get(),
                guideme.unsure = *unsure,
                guideme.yes_above = t.yes_above(),
                guideme.no_below = t.no_below(),
                guideme.min_confidence = t.min_confidence(),
            "{} score: level {index}",
            id.as_str(),
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
    /// Timeout per attempt; default 30 s.
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
