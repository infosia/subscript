//! Discovery for the runtime-trap corpus category.

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::SourceFile;

/// The corpus trap directory.
pub fn corpus_trap() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/trap")
}

/// Every trap entry id, sorted.
///
/// The trap category contains only single-file `.ts` entries and no
/// `.expected` files. Rejecting every other directory member makes the
/// returned count an exact enumeration of the category, rather than a
/// count that silently ignores unrecognized files.
pub fn trap_ids(trap: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(trap).expect("read corpus/trap") {
        let entry = entry.expect("read corpus/trap entry");
        assert!(
            entry.path().is_file(),
            "{}: trap corpus entries must be files",
            entry.path().display()
        );
        let name = entry.file_name().to_string_lossy().into_owned();
        let id = name
            .strip_suffix(".ts")
            .unwrap_or_else(|| panic!("{name}: trap corpus contains only .ts entries"));
        assert!(
            id.starts_with('t'),
            "{name}: trap corpus ids must start with `t`"
        );
        ids.push(id.to_string());
    }
    ids.sort();
    ids
}

/// Loads one single-file trap corpus entry.
pub fn trap_sources(trap: &Path, id: &str) -> Vec<SourceFile> {
    let path = trap.join(format!("{id}.ts"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read trap entry {}: {e}", path.display()));
    vec![SourceFile::new(format!("{id}.ts"), text)]
}
