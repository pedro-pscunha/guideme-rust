#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::path::Path;

use guideme::Error;
use guideme::api::Answer;
use guideme::policy::{Outcome, Thresholds, resolve};

#[test]
fn committed_spec_matches_render() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec");
    let rendered = guideme::spec::render()?;
    assert_eq!(rendered.len(), 6);
    for (rel, contents) in rendered {
        let on_disk = std::fs::read_to_string(root.join(&rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}; run `mise run spec`"));
        assert_eq!(
            on_disk, contents,
            "{rel} drifted; run `mise run spec` and commit"
        );
        if rel.ends_with("policy.json") {
            let vectors: Vec<serde_json::Value> = serde_json::from_str(&contents)?;
            assert_eq!(vectors.len(), guideme::spec::EXPECTED_VECTORS);
            assert!(
                vectors
                    .iter()
                    .all(|v| v.get("outcome").is_some() ^ v.get("error").is_some())
            );
            for v in &vectors {
                let answer: Answer = serde_json::from_value(v["answer"].clone())?;
                let t: Thresholds = serde_json::from_value(v["thresholds"].clone())?;
                match (resolve(&answer, t), v.get("outcome")) {
                    (Ok(got), Some(want)) => {
                        let want: Outcome = serde_json::from_value(want.clone())?;
                        assert_eq!(got, want, "{rel}: vector drifted");
                    }
                    (Err(Error::Protocol { .. }), None) => assert_eq!(v["error"], "protocol"),
                    (r, o) => panic!("{rel}: {r:?} vs {o:?}"),
                }
            }
        }
        if rel.ends_with("rubric.json") {
            let cases: Vec<serde_json::Value> = serde_json::from_str(&contents)?;
            assert_eq!(cases.len(), 7, "{rel}: the rubric contract may not shrink");
            for case in &cases {
                // A case with no parts renders to its rubric itself: what keeps every
                // rubric written before 0.1.1 on the wire unchanged.
                if case["examples"] == serde_json::json!([])
                    && case["counterexamples"] == serde_json::json!([])
                {
                    assert_eq!(case["rendered"], case["what"], "{rel}: passthrough broke");
                }
            }
        }
    }
    Ok(())
}
