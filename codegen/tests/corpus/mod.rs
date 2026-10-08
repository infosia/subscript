//! Corpus access shared by the integration tests.
//!
//! The entry set is always *derived* from `corpus/accept/`: single-file
//! `<id>.ts` entries and multi-file `<id>/` directories, with the
//! comparison set being every committed `<id>.expected`. Nothing here
//! names an entry, so adding a corpus entry or a golden changes no test
//! code (`specs/blocks/compiler.md` §2).

#[path = "../../../compiler/tests/corpus/interop.rs"]
#[allow(dead_code)]
pub(crate) mod interop;

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::SourceFile;

/// The corpus accept directory.
pub fn corpus_accept() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept")
}

/// True when the source names a declaration from a committed interop mirror.
pub(crate) fn references_interop(src: &str) -> bool {
    !interop::mirrors_for(src, |_, _| ()).is_empty()
}

/// Every entry id present in `accept`, single- and multi-file.
pub fn entry_ids(accept: &Path) -> Vec<String> {
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
    let mut sources: Vec<SourceFile> = if dir.is_dir() {
        let mut names: Vec<String> = fs::read_dir(&dir)
            .expect("read entry dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".ts"))
            .collect();
        names.sort();
        let entry = "main.ts".to_owned();
        assert!(
            names.contains(&entry),
            "{id}: directory entry requires main.ts"
        );
        let source = SourceFile::new(
            &entry,
            fs::read_to_string(dir.join(&entry)).expect("read entry"),
        );
        let discovered = subscript_compiler::discover_module_sources(
            (entry, source),
            |_, specifier| {
                let name = format!("{}.ts", specifier.trim_start_matches("./"));
                if !names.contains(&name) {
                    return Ok(None);
                }
                let text = fs::read_to_string(dir.join(&name)).expect("read source");
                Ok(Some((name.clone(), SourceFile::new(name, text))))
            },
            |_, diagnostics| diagnostics,
        )
        .expect("discover corpus modules");
        for name in &names {
            assert!(
                discovered.iter().any(|source| &source.name == name),
                "{id}: unreachable corpus source {name}"
            );
        }
        discovered
    } else {
        let path = accept.join(format!("{id}.ts"));
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        vec![SourceFile::new(format!("{id}.ts"), text)]
    };
    if sources.len() == 1
        && sources[0]
            .source
            .lines()
            .any(|line| line == "// corpus-ambient: yes")
    {
        sources[0].dts = true;
        sources[0].entry = false;
        sources.push(SourceFile::entry(
            "main.ts",
            "export function main(): void {}",
        ));
    }
    let text = sources
        .iter()
        .map(|source| source.source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    sources.splice(0..0, interop::mirrors_for(&text, SourceFile::ambient));
    sources
}
