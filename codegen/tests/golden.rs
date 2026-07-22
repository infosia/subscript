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
    // Corpus entries: single-file `<id>.ts` plus multi-file `<id>/`
    // directories.
    let mut entry_ids: Vec<String> = Vec::new();
    let mut golden_ids: Vec<String> = Vec::new();
    for e in fs::read_dir(&accept).expect("read corpus/accept").flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path().is_dir() {
            entry_ids.push(name);
        } else if let Some(id) = name.strip_suffix(".ts") {
            entry_ids.push(id.to_string());
        } else if let Some(id) = name.strip_suffix(".expected") {
            golden_ids.push(id.to_string());
        }
    }
    golden_ids.sort();
    // The set is derived, never pinned: every committed golden is
    // compared, today (a01–a21) and after the a22–a24 capture, with
    // no edits here. The floor guards against silently comparing an
    // empty set; goldens are never deleted (compiler.md §2).
    assert!(
        golden_ids.len() >= 21,
        "expected at least the 21 authored goldens, found {}",
        golden_ids.len()
    );

    let mut failures = Vec::new();
    let mut compared = 0usize;
    for id in &golden_ids {
        if !entry_ids.contains(id) {
            failures.push(format!("{id}: golden has no corpus entry"));
            continue;
        }
        let golden = fs::read(accept.join(format!("{id}.expected"))).expect("read golden");
        let sources = entry_sources(&accept, id);
        match run_jit(&sources) {
            Ok(bytes) => {
                compared += 1;
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
        "{} golden failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        compared,
        golden_ids.len(),
        "every committed golden must be compared (no silent skips)"
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
