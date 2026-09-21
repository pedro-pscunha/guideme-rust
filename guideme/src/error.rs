//! The one error type every public function returns.

use std::time::Duration;

/// Everything that can go wrong, from the wire to the policy.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// `401`: missing or invalid API key.
    #[error("unauthorized: missing or invalid TypeSafe API key")]
    Auth,
    /// `422`: the request body failed validation. `detail` is the API's own message.
    #[error("invalid request: {detail}")]
    Invalid {
        /// The API's description of the offending field.
        detail: String,
    },
    /// `429` after every retry was used.
    #[error("rate limited (retry-after: {retry_after:?})")]
    RateLimited {
        /// The last `retry-after` the API sent, if any.
        retry_after: Option<Duration>,
    },
    /// `529` after every retry was used.
    #[error("TypeSafe is overloaded")]
    Overloaded,
    /// Connection, TLS, timeout, or body-read failure.
    #[error("transport failure: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A status the API contract does not define.
    #[error("unexpected status {status}: {body}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Response body, verbatim.
        body: String,
    },
    /// The response violated the contract: undecodable body, answer kind mismatch, unknown
    /// option or level, probability outside `0..=1`, missing answer for a question.
    #[error("protocol violation: {detail}")]
    Protocol {
        /// What was violated.
        detail: String,
    },
    /// The policy labelled the answer unsure and no fallback was given.
    #[error("unsure answer for question {question}: {value} against threshold {threshold}")]
    Unsure {
        /// Question id (`q0`, `q1`, …) in encode order; the same id the wire and the span use.
        question: String,
        /// The probability (noul) or confidence (choice, score) that was judged.
        value: f64,
        /// The boundary the value fell short of: the nearer band edge for noul,
        /// `min_confidence` for choice and score.
        threshold: f64,
    },
    /// Bad configuration: thresholds outside `0..=1`, `no_below > yes_above`, missing key,
    /// empty batch, unserialisable state, empty or duplicate rubric.
    #[error("configuration error: {detail}")]
    Config {
        /// What was wrong.
        detail: String,
    },
}
