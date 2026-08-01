//! Writes every checker- and corpus-derived document under generated-docs.

use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let generated = repository_root.join("generated-docs");
    fs::create_dir_all(&generated)?;

    let documents = [
        (
            "api-reference.md",
            subscript_compiler::api_reference::render_markdown(),
        ),
        (
            "language-reference.md",
            subscript_compiler::language_reference::render_language_reference(&repository_root)?,
        ),
        (
            "corpus-index.md",
            subscript_compiler::language_reference::render_corpus_index(&repository_root)?,
        ),
    ];
    for (name, contents) in documents {
        let output = generated.join(name);
        fs::write(&output, contents)?;
        println!("wrote {}", output.display());
    }
    Ok(())
}
