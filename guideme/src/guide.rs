//! The product: one verb, `ask`, over any question shape.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tracing::{Instrument, field, info_span};

use crate::api::{Client, ModelInfo, QuestionId, Request, Usage};
use crate::ask::{Ask, Plan, Reply};
use crate::policy::{self, Outcome, Thresholds, Verdict};
use crate::{ApiKey, Error, Model, Policy, State};

const TARGET: &str = "guideme";

/// An answer with what the request cost and which model produced it.
///
/// [`Guide::ask`] is this with everything but the answer dropped; [`Guide::ask_with_receipt`]
/// keeps it. The two fields beside the answer are what the TypeSafe docs tell you to log: the
/// billed token count, and the versioned id to pin your thresholds to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt<T> {
    /// The answer, in the shape that was asked for.
    pub answer: T,
    /// The versioned model that answered, for example `jev-1.13.0`, even when an alias such as
    /// `jev-latest` was requested.
    pub model: Model,
    /// Tokens read and written. Input tokens are the billed ones.
    pub usage: Usage,
}

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
    transport: Transport,
    client: Option<Client>,
    model: Model,
    policy: Policy,
    record_state: bool,
}

/// The settings that build a [`Client`], and that an injected one already carries.
///
/// They live in their own struct so the two places that care — the builder that fills them in
/// and [`GuideBuilder::build`], which refuses them beside `client(..)` — name the same list
/// once each. `build` destructures it exhaustively, so adding a sixth setter without deciding
/// what it means beside an injected client is a compile error rather than a silent omission
/// from the refusal.
#[derive(Debug, Default)]
struct Transport {
    api_key: Option<ApiKey>,
    base_url: Option<String>,
    max_retries: Option<u32>,
    backoff: Option<Duration>,
    timeout: Option<Duration>,
}

impl Transport {
    /// The settings that were set, by the name of the setter that sets them.
    fn named(&self) -> Vec<&'static str> {
        let Self {
            api_key,
            base_url,
            max_retries,
            backoff,
            timeout,
        } = self;
        [
            ("api_key", api_key.is_some()),
            ("base_url", base_url.is_some()),
            ("max_retries", max_retries.is_some()),
            ("backoff", backoff.is_some()),
            ("timeout", timeout.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, was_set)| was_set.then_some(name))
        .collect()
    }

    /// Build the client these settings describe, leaving every unset one at its default.
    fn build(self) -> Result<Client, Error> {
        let Self {
            api_key,
            base_url,
            max_retries,
            backoff,
            timeout,
        } = self;
        let key = api_key.ok_or_else(|| Error::Config {
            detail: "api_key is required".into(),
        })?;
        let mut client = Client::builder(key);
        if let Some(n) = max_retries {
            client = client.max_retries(n);
        }
        if let Some(d) = backoff {
            client = client.backoff(d);
        }
        if let Some(d) = timeout {
            client = client.timeout(d);
        }
        if let Some(url) = base_url {
            client = client.base_url(url);
        }
        client.build()
    }
}

impl Guide {
    /// Read `TYPESAFE_API_KEY` (required), `TYPESAFE_BASE_URL` and `GUIDEME_MODEL` (optional).
    ///
    /// The one-liner. [`GuideBuilder::from_env`] is the same reading on a builder you are
    /// still configuring.
    pub fn from_env() -> Result<Self, Error> {
        Self::builder().from_env()?.build()
    }

    /// Start configuring a guide.
    pub fn builder() -> GuideBuilder {
        GuideBuilder {
            transport: Transport::default(),
            client: None,
            model: Model::latest(),
            policy: Policy::new(),
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
        self.ask_with_receipt(ask, state)
            .await
            .map(|receipt| receipt.answer)
    }

    /// [`ask`](Guide::ask), keeping what the response said about itself: which versioned model
    /// answered and what the call cost.
    ///
    /// Same request, same span, same fields; `ask` is this with everything but the answer
    /// dropped. Reach for it to attribute cost, or to pin thresholds to the model version that
    /// produced them.
    pub async fn ask_with_receipt<A: Ask>(
        &self,
        ask: A,
        state: impl Into<State>,
    ) -> Result<Receipt<A::Out>, Error> {
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
        let answer = decoded.inspect_err(|e| fail(&span, e))?;
        Ok(Receipt {
            answer,
            model: response.model,
            usage: response.usage,
        })
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
        | Error::Overloaded { .. }
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
    /// Read `TYPESAFE_API_KEY` (required), `TYPESAFE_BASE_URL` and `GUIDEME_MODEL` (optional),
    /// and leave everything else on this builder alone.
    ///
    /// `Guide::builder().from_env()?.policy(HOUSE).build()?` is the shape this exists for.
    /// [`Guide::from_env`] is the one-liner when there is nothing else to set.
    ///
    /// # Errors
    /// [`Error::Config`] when `TYPESAFE_API_KEY` is not set.
    pub fn from_env(self) -> Result<Self, Error> {
        let key = std::env::var("TYPESAFE_API_KEY").map_err(|_| Error::Config {
            detail: "TYPESAFE_API_KEY is not set".into(),
        })?;
        let mut builder = self.api_key(ApiKey::from(key));
        if let Ok(url) = std::env::var("TYPESAFE_BASE_URL") {
            builder = builder.base_url(url);
        }
        if let Ok(model) = std::env::var("GUIDEME_MODEL") {
            builder = builder.model(Model::new(model));
        }
        Ok(builder)
    }
    /// The API key. Required unless [`from_env`](GuideBuilder::from_env) or
    /// [`client`](GuideBuilder::client) supplies one.
    pub fn api_key(mut self, key: ApiKey) -> Self {
        self.transport.api_key = Some(key);
        self
    }
    /// Override the API origin.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.transport.base_url = Some(url.into());
        self
    }
    /// Send through this [`Client`] instead of building one from the settings below.
    ///
    /// Use it to hand in a configured transport — a proxy, a client certificate, a shared
    /// connection pool — through [`api::ClientBuilder::http`](crate::api::ClientBuilder::http).
    /// The client already carries the API key, the base URL, the retry budget, the backoff and
    /// the timeout, so setting any of those here as well is [`Error::Config`] at build time
    /// rather than a setting that quietly does nothing.
    ///
    /// [`from_env`](GuideBuilder::from_env) sets two of them — `api_key` always, `base_url`
    /// when `TYPESAFE_BASE_URL` is present — so it does not combine with this. An injected
    /// client is where the key belongs in that case: `api::Client::builder(key)`.
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
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
    /// Retries for `429`, `529` and a failure to connect; default 3.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.transport.max_retries = Some(n);
        self
    }
    /// Base delay for exponential backoff; default 500 ms. The wait is `backoff * 2^attempt`
    /// plus jitter, capped at 30 s, and a `retry-after` the API sends wins over it.
    pub fn backoff(mut self, d: Duration) -> Self {
        self.transport.backoff = Some(d);
        self
    }
    /// Timeout per attempt; default 30 s.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.transport.timeout = Some(d);
        self
    }
    /// Record the state JSON on the span. Off by default: state is user data.
    pub fn record_state(mut self, on: bool) -> Self {
        self.record_state = on;
        self
    }
    /// Build. Validates the policy now so a bad house policy fails at startup.
    pub fn build(self) -> Result<Guide, Error> {
        let client = if let Some(client) = self.client {
            let ignored = self.transport.named();
            if !ignored.is_empty() {
                return Err(Error::Config {
                    detail: format!(
                        "client(..) already carries the API key, the base URL and the retry, backoff and timeout settings, so {} would do nothing here; set it on api::Client::builder instead. from_env() sets api_key, and base_url when TYPESAFE_BASE_URL is present",
                        ignored.join(", ")
                    ),
                });
            }
            client
        } else {
            self.transport.build()?
        };
        self.policy.settle()?;
        Ok(Guide {
            inner: Arc::new(Inner {
                client,
                model: self.model,
                policy: self.policy,
                record_state: self.record_state,
            }),
        })
    }
}
