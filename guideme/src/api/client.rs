//! The only module that touches HTTP. Retries `429`/`529` with exponential backoff,
//! honouring `retry-after`, and maps statuses to [`Error`].
//!
//! Every attempt is one span shaped by the OpenTelemetry HTTP client conventions, so a
//! retried request shows up as sibling spans under the caller's span, each with its own
//! status code. A throttled attempt also emits a `guideme.retry` warning.

use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use tracing::{Instrument, Span, field, info_span};

use super::{ModelInfo, ModelsResponse, Request, Response};
use crate::{ApiKey, Error};

const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const JITTER_MS: u64 = 250;
const TARGET: &str = "guideme::api";

/// A client for the TypeSafe HTTP API. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    host: String,
    port: u16,
    api_key: ApiKey,
    max_retries: u32,
    backoff: Duration,
}

/// Configures a [`Client`].
#[derive(Debug)]
pub struct ClientBuilder {
    api_key: ApiKey,
    base_url: String,
    max_retries: u32,
    backoff: Duration,
    timeout: Option<Duration>,
    http: Option<reqwest::Client>,
}

impl Client {
    /// Defaults: `https://api.typesafe.ai`, 3 retries, 500 ms base backoff, 30 s timeout.
    pub fn new(api_key: ApiKey) -> Result<Self, Error> {
        Self::builder(api_key).build()
    }

    /// Start configuring a client.
    pub fn builder(api_key: ApiKey) -> ClientBuilder {
        ClientBuilder {
            api_key,
            base_url: DEFAULT_BASE_URL.to_owned(),
            max_retries: 3,
            backoff: Duration::from_millis(500),
            timeout: None,
            http: None,
        }
    }

    /// The host and port every request goes to.
    pub(crate) fn server(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    /// `POST /v1/systemone`.
    ///
    /// A `retry-after` longer than 30 s is not waited for: the call fails with
    /// [`Error::RateLimited`] or [`Error::Overloaded`] carrying that duration, so the caller
    /// decides.
    pub async fn evaluate(&self, request: &Request) -> Result<Response, Error> {
        let url = format!("{}/v1/systemone", self.base_url);
        let body = serde_json::to_vec(request).map_err(|e| Error::Config {
            detail: format!("request is not serialisable: {e}"),
        })?;
        self.retrying(
            || {
                info_span!(
                    target: TARGET,
                    "POST /v1/systemone",
                    otel.kind = "client",
                    http.request.method = "POST",
                    server.address = self.host.as_str(),
                    server.port = i64::from(self.port),
                    url.full = url.as_str(),
                    url.template = "/v1/systemone",
                    http.request.resend_count = field::Empty,
                    http.response.status_code = field::Empty,
                    error.type = field::Empty,
                    otel.status_code = field::Empty,
                )
            },
            || {
                self.http
                    .post(&url)
                    .bearer_auth(self.api_key.expose())
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body.clone())
            },
        )
        .await
    }

    /// `GET /v1/models`. Retried on the same statuses, in the same budget, as [`evaluate`].
    ///
    /// [`evaluate`]: Client::evaluate
    pub async fn models(&self) -> Result<Vec<ModelInfo>, Error> {
        let url = format!("{}/v1/models", self.base_url);
        let body: ModelsResponse = self
            .retrying(
                || {
                    info_span!(
                        target: TARGET,
                        "GET /v1/models",
                        otel.kind = "client",
                        http.request.method = "GET",
                        server.address = self.host.as_str(),
                        server.port = i64::from(self.port),
                        url.full = url.as_str(),
                        url.template = "/v1/models",
                        http.request.resend_count = field::Empty,
                        http.response.status_code = field::Empty,
                        error.type = field::Empty,
                        otel.status_code = field::Empty,
                    )
                },
                || self.http.get(&url).bearer_auth(self.api_key.expose()),
            )
            .await?;
        Ok(body.models)
    }

    /// One request, retried. Both endpoints drive this, so the budget, the backoff and the
    /// span shape are one implementation: `new_span` names the attempt — a span name is static
    /// metadata, so each endpoint writes its own — and `new_request` builds the request again,
    /// because sending one consumes it.
    ///
    /// Retried: `429`, `529`, and whatever `reqwest` reports as a failure to connect —
    /// refused, reset, a TLS handshake. Not retried: a timeout of any phase, or a body
    /// failure. [`ClientBuilder`] sets one overall deadline per attempt and never a
    /// `connect_timeout`, under which `reqwest` classifies a connect-phase timeout as
    /// `is_timeout()` rather than `is_connect()` — so no timeout is retryable here, and
    /// retrying one would multiply the wall time that deadline promises. A client handed in
    /// through [`ClientBuilder::http`] brings its own classification: a `connect_timeout` set
    /// on it does make connect timeouts retryable.
    async fn retrying<T, S, B>(&self, new_span: S, new_request: B) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
        S: Fn() -> Span,
        B: Fn() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0;
        loop {
            let last = attempt == self.max_retries;
            let span = new_span();
            if attempt > 0 {
                span.record("http.request.resend_count", i64::from(attempt));
            }
            let sent = new_request().send().instrument(span.clone()).await;
            // `status` is `None` when the attempt never reached a server, which is what the
            // retry event reports in place of a status code it cannot know.
            let (status, delay) = match sent {
                Err(e) => {
                    let reached_nobody = e.is_connect();
                    let e = transport(e);
                    fail(&span, e.kind());
                    if last || !reached_nobody {
                        return Err(e);
                    }
                    (None, self.delay(attempt))
                }
                Ok(response) => {
                    let status = response.status().as_u16();
                    span.record("http.response.status_code", i64::from(status));
                    if status == 200 {
                        let decoded = decode::<T>(response).instrument(span.clone()).await;
                        if let Err(e) = &decoded {
                            fail(&span, e.kind());
                        }
                        return decoded;
                    }
                    fail(&span, &status.to_string());
                    if !matches!(status, 429 | 529) {
                        return Err(classify(status, response).await);
                    }
                    let retry_after = parse_retry_after(&response);
                    if last || retry_after.is_some_and(|d| d > MAX_BACKOFF) {
                        return Err(throttled(status, retry_after));
                    }
                    (
                        Some(status),
                        retry_after.unwrap_or_else(|| self.delay(attempt)),
                    )
                }
            };
            warn_retry(&span, status, attempt, delay);
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }

    /// Exponential backoff with jitter: `base * 2^attempt`, capped, plus up to 250 ms.
    fn delay(&self, attempt: u32) -> Duration {
        let exp = self
            .backoff
            .saturating_mul(2u32.saturating_pow(attempt))
            .min(MAX_BACKOFF);
        let jitter_ms = RandomState::new().hash_one(attempt) % (JITTER_MS + 1);
        exp + Duration::from_millis(jitter_ms)
    }
}

impl ClientBuilder {
    /// Override the API origin (tests, proxies). No trailing slash.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        let mut url: String = url.into();
        while url.ends_with('/') {
            url.pop();
        }
        self.base_url = url;
        self
    }
    /// Retries for `429`/`529`. `0` disables retrying.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }
    /// Base delay for exponential backoff.
    pub fn backoff(mut self, d: Duration) -> Self {
        self.backoff = d;
        self
    }
    /// Timeout per attempt; default 30 s. Worst case wall time is `(max_retries + 1) × timeout`
    /// plus backoff.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
    /// Send through this `reqwest` client instead of one built here: a proxy, a client
    /// certificate, a connection pool shared with the rest of the application, or a transport
    /// that answers without a network.
    ///
    /// The deadline belongs to the client handed in, so setting
    /// [`timeout`](ClientBuilder::timeout) as well is [`Error::Config`] at build time rather
    /// than a silent override. Everything else — the base URL, the retry budget and the
    /// backoff — is guideme's and still applies.
    pub fn http(mut self, client: reqwest::Client) -> Self {
        self.http = Some(client);
        self
    }
    /// Build the client. Fails if the base URL has no host or port, or carries credentials:
    /// the URL is recorded on every request span as `url.full`, which must never hold a
    /// secret. Also fails when a timeout is set beside an injected client.
    pub fn build(self) -> Result<Client, Error> {
        let http = match self.http {
            Some(http) => {
                if self.timeout.is_some() {
                    return Err(Error::Config {
                        detail: "timeout(..) and http(..) both set the request deadline; set it on the reqwest client you hand to http(..)".into(),
                    });
                }
                http
            }
            None => reqwest::Client::builder()
                .timeout(self.timeout.unwrap_or(DEFAULT_TIMEOUT))
                .build()
                .map_err(transport)?,
        };
        let parsed = reqwest::Url::parse(&self.base_url).map_err(|e| Error::Config {
            detail: format!("base_url {:?}: {e}", self.base_url),
        })?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(Error::Config {
                detail: "base_url must not carry credentials; use the api_key".into(),
            });
        }
        let (Some(host), Some(port)) = (parsed.host_str(), parsed.port_or_known_default()) else {
            return Err(Error::Config {
                detail: format!("base_url {:?} needs a host and a port", self.base_url),
            });
        };
        Ok(Client {
            http,
            host: host.to_owned(),
            port,
            base_url: self.base_url,
            api_key: self.api_key,
            max_retries: self.max_retries,
            backoff: self.backoff,
        })
    }
}

/// Marks an attempt span as failed. `error_type` is the status code when a response arrived,
/// otherwise the [`Error::kind`].
fn fail(span: &Span, error_type: &str) {
    span.record("error.type", error_type);
    span.record("otel.status_code", "ERROR");
}

/// The typed error a throttle becomes once the budget is spent, carrying whatever
/// `retry-after` the API last sent.
fn throttled(status: u16, retry_after: Option<Duration>) -> Error {
    if status == 429 {
        Error::RateLimited { retry_after }
    } else {
        Error::Overloaded { retry_after }
    }
}

/// The `guideme.retry` warning, emitted inside the failed attempt's span just before the wait.
///
/// Exactly one of `http.response.status_code` and `error.type` is on it: the status when a
/// response arrived, `transport` when the attempt never reached a server. They are two
/// callsites because a `tracing` event's field set is static metadata.
fn warn_retry(span: &Span, status: Option<u16>, attempt: u32, delay: Duration) {
    let ordinal = i64::from(attempt) + 1;
    let delay_ms = i64::try_from(delay.as_millis()).unwrap_or(i64::MAX);
    span.in_scope(|| {
        if let Some(status) = status {
            tracing::event!(
                name: "guideme.retry",
                target: TARGET,
                tracing::Level::WARN,
                http.response.status_code = i64::from(status),
                guideme.retry.attempt = ordinal,
                guideme.retry.delay_ms = delay_ms,
                "{status} from TypeSafe, retrying in {} ms",
                delay.as_millis(),
            );
        } else {
            tracing::event!(
                name: "guideme.retry",
                target: TARGET,
                tracing::Level::WARN,
                error.type = "transport",
                guideme.retry.attempt = ordinal,
                guideme.retry.delay_ms = delay_ms,
                "could not reach TypeSafe, retrying in {} ms",
                delay.as_millis(),
            );
        }
    });
}

/// Non-retryable statuses: `401` and `422` are typed, anything else is unexpected.
async fn classify(status: u16, response: reqwest::Response) -> Error {
    let body = match response.text().await {
        Ok(text) => text,
        Err(e) => return transport(e),
    };
    match status {
        401 => Error::Auth,
        422 => Error::Invalid { detail: body },
        other => Error::UnexpectedStatus {
            status: other,
            body,
        },
    }
}

/// Reads a `200` body and decodes it; a body that violates the contract is [`Error::Protocol`].
async fn decode<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T, Error> {
    let body = response.text().await.map_err(transport)?;
    serde_json::from_str(&body).map_err(|e| Error::Protocol {
        detail: format!("response body: {e}"),
    })
}

fn transport(e: reqwest::Error) -> Error {
    Error::Transport(Box::new(e))
}

fn parse_retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}
