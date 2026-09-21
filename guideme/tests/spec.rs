#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::path::Path;

#[test]
fn committed_spec_matches_render() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec");
    let rendered = guideme::spec::render()?;
    assert_eq!(rendered.len(), 5);
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
        }
    }
    Ok(())
}
