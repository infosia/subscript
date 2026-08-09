//! Regenerates the embedding host's C header from the Rust ABI
//! declarations.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/subscript_runtime.h");
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, subscript_runtime::host_header::render()?)?;
    println!("wrote {}", output.display());
    Ok(())
}
