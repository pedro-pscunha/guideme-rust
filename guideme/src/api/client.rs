//! The only module that touches HTTP. Retries `429`/`529` with exponential backoff,
//! honouring `retry-after`, and maps statuses to [`Error`].

use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use super::{ModelInfo, ModelsResponse, Request, Response};
use crate::{ApiKey, Error};

const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const JITTER_MS: u64 = 250;

/// A client for the TypeSafe HTTP API. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
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
    timeout: Duration,
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
            timeout: Duration::from_secs(30),
        }
    }

    /// `POST /v1/systemone`.
    pub async fn evaluate(&self, request: &Request) -> Result<Response, Error> {
        self.evaluate_counted(request)
            .await
            .map(|(response, _)| response)
    }

    /// `POST /v1/systemone`, also reporting how many retries were spent.
    ///
    /// A `retry-after` longer than 30 s is not waited for: the call fails with
    /// [`Error::RateLimited`] carrying that duration, so the caller decides.
    pub(crate) async fn evaluate_counted(
        &self,
        request: &Request,
    ) -> Result<(Response, u32), Error> {
        let url = format!("{}/v1/systemone", self.base_url);
        let body = serde_json::to_vec(request).map_err(|e| Error::Config {
            detail: format!("request is not serialisable: {e}"),
        })?;
        let mut retry_after = None;
        for attempt in 0..=self.max_retries {
            let response = self
                .http
                .post(&url)
                .bearer_auth(self.api_key.expose())
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone())
                .send()
                .await
                .map_err(transport)?;
            let status = response.status().as_u16();
            match status {
                200 => return decode::<Response>(response).await.map(|r| (r, attempt)),
                429 | 529 => {
                    retry_after = parse_retry_after(&response);
                    if attempt == self.max_retries || retry_after.is_some_and(|d| d > MAX_BACKOFF) {
                        return Err(if status == 429 {
                            Error::RateLimited { retry_after }
                        } else {
                            Error::Overloaded
                        });
                    }
                    tokio::time::sleep(retry_after.unwrap_or_else(|| self.delay(attempt))).await;
                }
                other => return Err(classify(other, response).await),
            }
        }
        Err(Error::RateLimited { retry_after })
    }

    /// `GET /v1/models`. Not retried.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, Error> {
        let url = format!("{}/v1/models", self.base_url);
        let response = self
            .http
            .get(&url)
            .bearer_auth(self.api_key.expose())
            .send()
            .await
            .map_err(transport)?;
        match response.status().as_u16() {
            200 => Ok(decode::<ModelsResponse>(response).await?.models),
            429 => Err(Error::RateLimited {
                retry_after: parse_retry_after(&response),
            }),
            529 => Err(Error::Overloaded),
            other => Err(classify(other, response).await),
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
    /// Timeout per attempt. Worst case wall time is `(max_retries + 1) × timeout` plus backoff.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }
    /// Build the client.
    pub fn build(self) -> Result<Client, Error> {
        let http = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(transport)?;
        Ok(Client {
            http,
            base_url: self.base_url,
            api_key: self.api_key,
            max_retries: self.max_retries,
            backoff: self.backoff,
        })
    }
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
