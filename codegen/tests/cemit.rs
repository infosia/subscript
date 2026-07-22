//! Verification for the P4.2 C-emission spike
//! (`specs/tracking/p4-performance.md`, `specs/blocks/compiler.md` §9).
//!
//! The emitted-C program for the a22 corpus entry, compiled with the
//! platform C compiler at `-O2 -ffp-contract=off`, must print the frozen
//! golden byte-exactly. A mismatch means the emitter is wrong (not the
//! golden), so this test is the spike's correctness gate.

use std::path::PathBuf;
use std::process::Command;

use subscript_codegen::emit_c;
use subscript_compiler::{check_program, SourceFile};

/// The a22 corpus entry and its frozen golden, compiled into the test so
/// the measured program is exactly the committed file.
const A22_SOURCE: &str = include_str!("../../corpus/accept/a22-matrix-propagation.ts");
const A22_GOLDEN: &[u8] = include_bytes!("../../corpus/accept/a22-matrix-propagation.expected");

/// The C compiler driver and flags §9 pins (the same the P4 baseline
/// uses); `-ffp-contract=off` matches the language's f32 arithmetic.
fn host_cc() -> String {
    std::env::var("CC").unwrap_or_else(|_| "cc".to_string())
}

#[test]
fn emitted_c_for_a22_prints_the_frozen_golden_byte_exactly() {
    let files = vec![SourceFile::new("a22-matrix-propagation.ts", A22_SOURCE)];
    let module = check_program(&files).expect("a22 checks clean");
    let c_source = emit_c(&module).expect("a22 emits C");

    // Work in a unique temp dir outside the repository.
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "subscript-cemit-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = dir.join("a22-cemit.c");
    let exe = dir.join("a22-cemit");
    std::fs::write(&src, c_source.as_bytes()).expect("write C source");

    let build = Command::new(host_cc())
        .args(["-O2", "-ffp-contract=off"])
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run the platform C compiler");
    assert!(
        build.status.success(),
        "compiling the emitted C failed:\n{}\n--- emitted C ---\n{}",
        String::from_utf8_lossy(&build.stderr),
        c_source
    );

    // Run under the harness protocol (minimum counts); stdout is the
    // program output, which must equal the golden byte for byte.
    let run = Command::new(&exe)
        .arg("3")
        .arg("11")
        .output()
        .expect("run the emitted-C program");
    assert!(
        run.status.success(),
        "the emitted-C program exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        run.stdout,
        A22_GOLDEN,
        "emitted-C printed {:?}, golden is {:?}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(A22_GOLDEN)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
