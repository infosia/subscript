//! `subscript-bindgen` CLI (`specs/blocks/compiler.md` §13.5): the generic
//! `--header <path>` frontend. Direct end-to-end tests of the shipped
//! binary — usage/help, a clean mirror to stdout, and fail-loud (naming the
//! offending construct, nonzero exit) on a header the toolchain cannot map.

use std::path::PathBuf;
use std::process::Command;

/// The compiled CLI binary (cargo provides its path to integration tests).
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_subscript-bindgen"))
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn help_prints_usage_and_succeeds() {
    let out = bin().arg("--help").output().expect("run --help");
    assert!(out.status.success(), "--help must exit 0");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--header <path>"), "usage names --header: {text}");
}

#[test]
fn header_flag_emits_the_mirror_to_stdout() {
    let header = repo().join("corpus/interop/interop.h");
    let out = bin()
        .arg("--header")
        .arg(&header)
        .output()
        .expect("run --header");
    assert!(out.status.success(), "binding a clean header must exit 0");
    let text = String::from_utf8_lossy(&out.stdout);
    // A representative committed mirror line proves the real frontend ran.
    assert!(text.contains("GENERATED FILE"), "emits the mirror header: {text}");
    assert!(
        text.contains("declare function subDevicePump(device: SubDevice): void;"),
        "emits the P6.3 async entry: {text}"
    );
}

#[test]
fn positional_header_path_also_works() {
    let header = repo().join("corpus/interop/interop.h");
    let out = bin().arg(&header).output().expect("run positional");
    assert!(out.status.success(), "positional header path must exit 0");
    assert!(String::from_utf8_lossy(&out.stdout).contains("GENERATED FILE"));
}

#[test]
fn unmappable_construct_fails_loud_naming_it() {
    // A bare `long` field is target-width-dependent (LP64 vs LLP64) and
    // unmapped (emit.rs); the CLI must fail loud, name `long`, and exit
    // nonzero — never write a silently invalid mirror.
    let dir = std::env::temp_dir().join(format!("subscript-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let bad = dir.join("bad.h");
    std::fs::write(&bad, "typedef struct S { long n; } S;\nvoid f(S s);\n").expect("write");

    let out = bin().arg("--header").arg(&bad).output().expect("run bad header");
    assert!(!out.status.success(), "an unmappable header must exit nonzero");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("long"), "the error names the offending construct: {err}");
    assert!(out.stdout.is_empty(), "no mirror is written on failure");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_header_argument_is_an_error() {
    let out = bin().output().expect("run with no args");
    assert!(!out.status.success(), "no header path must exit nonzero");
}
