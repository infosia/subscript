//! Golden differential gate (compiler.md §2/§7): every committed
//! `corpus/accept/<id>.expected` must byte-match the dev-JIT output
//! of its entry, and the not-yet-captured entries (a22–a24) must run
//! to completion without a trap.

use std::fs;
use std::path::{Path, PathBuf};

use subscript_codegen::run_jit;
use subscript_compiler::SourceFile;

fn corpus_accept() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept")
}

/// Loads the source files of one corpus entry (a19 is the two-file
/// program in its own directory).
fn entry_sources(accept: &Path, id: &str) -> Vec<SourceFile> {
    let dir = accept.join(id);
    if dir.is_dir() {
        let mut names: Vec<String> = fs::read_dir(&dir)
            .expect("read entry dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".ts"))
            .collect();
        names.sort();
        // The entry file must come first so diagnostics and import
        // resolution treat it as the root; `main.ts` sorts after
        // `math.ts`, so order by "contains main" first.
        names.sort_by_key(|n| !n.contains("main"));
        names
            .iter()
            .map(|n| {
                let text = fs::read_to_string(dir.join(n)).expect("read source");
                SourceFile::new(n.clone(), text)
            })
            .collect()
    } else {
        let path = accept.join(format!("{id}.ts"));
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        vec![SourceFile::new(format!("{id}.ts"), text)]
    }
}

#[test]
fn every_committed_golden_matches_the_jit_byte_for_byte() {
    let accept = corpus_accept();
    let mut ids: Vec<String> = fs::read_dir(&accept)
        .expect("read corpus/accept")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".expected"))
        .map(|n| n.trim_end_matches(".expected").to_string())
        .collect();
    ids.sort();
    assert_eq!(ids.len(), 21, "expected 21 committed goldens (a01–a21)");

    let mut failures = Vec::new();
    for id in &ids {
        let golden = fs::read(accept.join(format!("{id}.expected"))).expect("read golden");
        let sources = entry_sources(&accept, id);
        match run_jit(&sources) {
            Ok(bytes) => {
                if bytes != golden {
                    failures.push(format!(
                        "{id}: JIT output {:?} != golden {:?}",
                        String::from_utf8_lossy(&bytes),
                        String::from_utf8_lossy(&golden)
                    ));
                }
            }
            Err(e) => failures.push(format!("{id}: run failed: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} golden mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn capture_set_a22_to_a24_runs_to_completion() {
    let accept = corpus_accept();
    for id in [
        "a22-matrix-propagation",
        "a23-game-loop",
        "a24-particle-system",
    ] {
        let sources = entry_sources(&accept, id);
        match run_jit(&sources) {
            Ok(bytes) => {
                assert!(!bytes.is_empty(), "{id}: produced no output");
                assert_eq!(
                    bytes.last(),
                    Some(&b'\n'),
                    "{id}: output does not end in a newline"
                );
            }
            Err(e) => panic!("{id}: run failed: {e}"),
        }
    }
}
