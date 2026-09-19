//! The p21 allocation-metadata goldens of `emit_c` (compiler.md §21).
//!
//! `codegen/tests/fixtures/p21-allocation-metadata.h` and
//! `p21-allocation-metadata.inc` are the emitter's output for
//! `corpus/accept/a15-manual-lifetime.ts`. The test compares them byte
//! for byte, and the capture branch writes them again from the
//! generator, so no person copies the text by hand (CLAUDE.md core
//! principle 6).

use std::path::{Path, PathBuf};

use subscript_codegen::emit_c;
use subscript_compiler::{check_program, SourceFile};

/// Set this variable to write both fixtures from the generator.
const CAPTURE: &str = "SUBSCRIPT_CAPTURE_ALLOC_METADATA_GOLDENS";

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Reads the committed bytes at run time, so a capture and the next run
/// need no rebuild.
fn compare(path: &Path, generated: &str) {
    let committed =
        std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    if generated.as_bytes() != committed.as_slice() {
        panic!(
            "{} drifted from emit_c: {} bytes committed, {} bytes generated. \
Set {CAPTURE}=1 to write the file again.\ngenerated:\n{generated}",
            path.display(),
            committed.len(),
            generated.len()
        );
    }
}

/// Cost: one `check_program` and one `emit_c` of a 23-line corpus entry,
/// plus two file reads. The test compiles and runs no C. Measured on
/// arm64 macOS, debug profile: 0.005 s for each run of the whole target
/// process, the mean of 20 runs. The harness prints `finished in 0.00s`.
#[test]
fn allocation_metadata_regenerates_byte_identically() {
    let source = include_str!("../../corpus/accept/a15-manual-lifetime.ts");
    let hir = check_program(&[SourceFile::new("a15-manual-lifetime.ts", source)])
        .expect("the a15 corpus entry checks cleanly");
    let program = emit_c(&hir).expect("the a15 corpus entry emits ship C");

    let header = fixture_path("p21-allocation-metadata.h");
    let tables = fixture_path("p21-allocation-metadata.inc");
    if std::env::var_os(CAPTURE).is_some() {
        std::fs::write(&header, program.allocation_metadata_header.as_bytes())
            .unwrap_or_else(|error| panic!("write {}: {error}", header.display()));
        std::fs::write(&tables, program.allocation_metadata_source.as_bytes())
            .unwrap_or_else(|error| panic!("write {}: {error}", tables.display()));
        return;
    }
    compare(&header, &program.allocation_metadata_header);
    compare(&tables, &program.allocation_metadata_source);
}
