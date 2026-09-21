#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use guideme::{ApiKey, Guide};

#[test]
fn debug_output_never_contains_the_api_key() {
    let sentinel = "redaction-sentinel-value";
    let key = ApiKey::from(sentinel);
    let guide = Guide::builder()
        .api_key(key.clone())
        .build()
        .expect("builder");

    let rendered = format!("{key:?} {guide:?}");
    assert!(!rendered.contains(sentinel));
    assert!(rendered.contains("ApiKey(***)"));
}
