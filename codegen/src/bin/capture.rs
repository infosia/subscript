//! Capture helper (compiler.md §2): runs one accept-corpus entry
//! under the dev JIT and writes the raw stdout bytes of the run to
//! this process's stdout. Exit status: 0 on normal completion, 1 on
//! trap or rejection, 2 on usage/IO errors.
//!
//! Usage: `cargo run --offline -p subscript-codegen --bin capture -- <entry-id>`
//! e.g. `capture a22-matrix-propagation`. The orchestrator redirects
//! stdout into the golden file after review; this tool never writes
//! `.expected` files itself.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use subscript_codegen::run_jit;
use subscript_compiler::SourceFile;

fn main() -> ExitCode {
    let Some(id) = std::env::args().nth(1) else {
        eprintln!("usage: capture <entry-id>   (e.g. capture a22-matrix-propagation)");
        return ExitCode::from(2);
    };
    let accept = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");

    let dir = accept.join(&id);
    let sources: Vec<SourceFile> = if dir.is_dir() {
        let mut names: Vec<String> = match fs::read_dir(&dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".ts"))
                .collect(),
            Err(e) => {
                eprintln!("capture: read {}: {e}", dir.display());
                return ExitCode::from(2);
            }
        };
        names.sort();
        names.sort_by_key(|n| !n.contains("main"));
        let mut out = Vec::new();
        for n in names {
            match fs::read_to_string(dir.join(&n)) {
                Ok(text) => out.push(SourceFile::new(n, text)),
                Err(e) => {
                    eprintln!("capture: read {n}: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        out
    } else {
        let path = accept.join(format!("{id}.ts"));
        match fs::read_to_string(&path) {
            Ok(text) => vec![SourceFile::new(format!("{id}.ts"), text)],
            Err(e) => {
                eprintln!("capture: read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    };

    match run_jit(&sources) {
        Ok(bytes) => {
            if std::io::stdout().write_all(&bytes).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("capture: {id}: {e}");
            ExitCode::FAILURE
        }
    }
}
