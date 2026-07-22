//! Corpus access shared by the integration tests.
//!
//! The entry set is always *derived* from `corpus/accept/`: single-file
//! `<id>.ts` entries and multi-file `<id>/` directories, with the
//! comparison set being every committed `<id>.expected`. Nothing here
//! names an entry, so adding a corpus entry or a golden changes no test
//! code (`specs/blocks/compiler.md` §2).

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::SourceFile;

/// The corpus accept directory.
pub fn corpus_accept() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept")
}

/// Every entry id present in `accept`, single- and multi-file.
fn entry_ids(accept: &Path) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for e in fs::read_dir(accept).expect("read corpus/accept").flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path().is_dir() {
            ids.push(name);
        } else if let Some(id) = name.strip_suffix(".ts") {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    ids
}

/// Every entry id that has a committed golden, sorted. Panics when a
/// golden has no corpus entry: a golden is never compared against
/// nothing.
pub fn golden_ids(accept: &Path) -> Vec<String> {
    let entries = entry_ids(accept);
    let mut ids: Vec<String> = Vec::new();
    for e in fs::read_dir(accept).expect("read corpus/accept").flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_suffix(".expected") {
            assert!(
                entries.contains(&id.to_string()),
                "{id}: golden has no corpus entry"
            );
            ids.push(id.to_string());
        }
    }
    ids.sort();
    ids
}

/// The committed golden bytes of `id`.
pub fn golden_bytes(accept: &Path, id: &str) -> Vec<u8> {
    let path = accept.join(format!("{id}.expected"));
    fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Loads the source files of one corpus entry (a multi-file entry is a
/// directory of `.ts` files).
pub fn entry_sources(accept: &Path, id: &str) -> Vec<SourceFile> {
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
        // resolution treat it as the root.
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
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        vec![SourceFile::new(format!("{id}.ts"), text)]
    }
}
