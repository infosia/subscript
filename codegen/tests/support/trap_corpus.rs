//! Discovery for the runtime-trap corpus category.

use std::fs;
use std::path::{Path, PathBuf};

use subscript_codegen::{
    run_c_aot_with_native_libraries_and_host_hooks, EntryArg, NativeLibrary, ReloadSession,
    RunError,
};
use subscript_compiler::SourceFile;

/// The corpus trap directory.
pub fn corpus_trap() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/trap")
}

/// Every trap entry id, sorted.
///
/// The trap category contains paired single-file `.ts` and `.expected`
/// entries. Rejecting every other directory member and incomplete pair
/// makes the returned count an exact enumeration of the category,
/// rather than a count that silently ignores unrecognized files.
pub fn trap_ids(trap: &Path) -> Vec<String> {
    let mut source_ids = Vec::new();
    let mut expected_ids = Vec::new();
    for entry in fs::read_dir(trap).expect("read corpus/trap") {
        let entry = entry.expect("read corpus/trap entry");
        assert!(
            entry.path().is_file(),
            "{}: trap corpus entries must be files",
            entry.path().display()
        );
        let name = entry.file_name().to_string_lossy().into_owned();
        let (id, ids) = if let Some(id) = name.strip_suffix(".ts") {
            (id, &mut source_ids)
        } else if let Some(id) = name.strip_suffix(".expected") {
            (id, &mut expected_ids)
        } else {
            panic!("{name}: trap corpus contains only paired .ts and .expected entries");
        };
        assert!(
            id.starts_with('t'),
            "{name}: trap corpus ids must start with `t`"
        );
        ids.push(id.to_string());
    }
    source_ids.sort();
    expected_ids.sort();
    assert_eq!(
        source_ids, expected_ids,
        "trap corpus .ts and .expected ids must be paired exactly"
    );
    source_ids
}

/// Loads one single-file trap corpus entry.
pub fn trap_sources(trap: &Path, id: &str) -> Vec<SourceFile> {
    let path = trap.join(format!("{id}.ts"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read trap entry {}: {e}", path.display()));
    let mut sources = vec![SourceFile::new(format!("{id}.ts"), text)];
    // The two narrowing probes receive their only checker-permitted
    // `object | null` values through the generated host-boundary mirror.
    // Keep the mirror ambient (not a second checked module), exactly as
    // the accept-corpus interop entries do.
    if sources[0].source.contains("SubCallbackInfo") {
        let mirror = trap
            .parent()
            .expect("corpus/trap has a corpus parent")
            .join("interop/interop.generated.d.ts");
        let text = fs::read_to_string(&mirror)
            .unwrap_or_else(|e| panic!("read trap ambient mirror {}: {e}", mirror.display()));
        sources.insert(0, SourceFile::ambient("interop.generated.d.ts", text));
    }
    if sources[0].source.contains("SubWireMode") {
        let mirror = trap
            .parent()
            .expect("corpus/trap has a corpus parent")
            .join("interop/wire-enum.generated.d.ts");
        let text = fs::read_to_string(&mirror)
            .unwrap_or_else(|e| panic!("read trap ambient mirror {}: {e}", mirror.display()));
        sources.insert(0, SourceFile::ambient("wire-enum.generated.d.ts", text));
        let aliases = trap
            .parent()
            .expect("corpus/trap has a corpus parent")
            .join("interop/wire-enum-aliases.d.ts");
        let text = fs::read_to_string(&aliases)
            .unwrap_or_else(|e| panic!("read trap ambient aliases {}: {e}", aliases.display()));
        sources.insert(0, SourceFile::ambient("wire-enum-aliases.d.ts", text));
    }
    sources
}

/// Loads the exact dev-tier stdout expected before a trap.
pub fn trap_expected(trap: &Path, id: &str) -> Vec<u8> {
    let path = trap.join(format!("{id}.expected"));
    fs::read(&path).unwrap_or_else(|e| panic!("read trap golden {}: {e}", path.display()))
}

/// Drives the R32 unknown wire value through the dev host-entry surface.
pub fn run_wire_entry_unknown_dev(
    sources: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
    session.call_export_with("configure", &[EntryArg::I32(12345), EntryArg::I32(5)])?;
    session.call_main()?;
    Ok(session.take_output())
}

/// Drives the R32 unknown wire value through the ship host-entry surface.
pub fn run_wire_entry_unknown_ship(
    sources: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    run_c_aot_with_native_libraries_and_host_hooks(
        sources,
        libraries,
        Some("subWireEntryDriveUnknown"),
        None,
    )
}
