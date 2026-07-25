//! Writes the checker-derived generated API compatibility reference.

use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../generated-docs/api-reference.md");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &output,
        subscript_compiler::api_reference::render_markdown(),
    )?;
    println!("wrote {}", output.display());
    Ok(())
}
