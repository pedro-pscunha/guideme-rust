//! Writes `guideme::spec::render()` into `spec/` at the workspace root.

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec");
    for (rel, contents) in guideme::spec::render()? {
        let path = root.join(&rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, contents)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
