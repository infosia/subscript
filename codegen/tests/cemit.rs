//! Ship-tier C-emission checks (`specs/blocks/compiler.md` §11).
//!
//! The full run-set differential (dev-JIT ≡ ship-C-AOT ≡ golden) lives
//! in `golden.rs`. This file pins two ship-tier properties directly:
//! the a22 checksum entry compiles, links, and prints the frozen golden
//! through the ship path, and a reachable trap the analysis cannot prove
//! away is reported with its kind and TS position (the trap model, §11).

use subscript_codegen::{run_c_aot, RunError};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

/// The a22 corpus entry and its frozen golden, compiled into the test so
/// the measured program is exactly the committed file.
const A22_SOURCE: &str = include_str!("../../corpus/accept/a22-matrix-propagation.ts");
const A22_GOLDEN: &[u8] = include_bytes!("../../corpus/accept/a22-matrix-propagation.expected");

#[test]
fn ship_c_aot_prints_the_frozen_a22_golden_byte_exactly() {
    let out = run_c_aot(&[SourceFile::new("a22-matrix-propagation.ts", A22_SOURCE)])
        .expect("a22 runs through the ship tier");
    assert_eq!(
        out,
        A22_GOLDEN,
        "ship-C-AOT printed {:?}, golden is {:?}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(A22_GOLDEN)
    );
}

#[test]
fn ship_c_aot_reports_an_out_of_bounds_trap_with_its_position() {
    // The index is a parameter — the FixedArray bounds analysis cannot
    // prove it in range, so the check stays and fires at the indexing
    // expression's TS position.
    let err = run_c_aot(&[SourceFile::new(
        "test.ts",
        "function at(xs: FixedArray<i32, 3>, i: i32): i32 {\n  return xs[i];\n}\nexport function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${at(xs, 5)}`);\n}\n",
    )]);
    match err {
        Err(RunError::Trap(t)) => {
            assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
            assert_eq!(t.pos.file, "test.ts");
            assert_eq!(t.pos.line, 2);
        }
        other => panic!("expected an out-of-bounds trap, got {other:?}"),
    }
}

#[test]
fn ship_c_aot_reports_a_division_by_zero_trap() {
    let err = run_c_aot(&[SourceFile::new(
        "test.ts",
        "function f(d: i32): i32 {\n  return 10 / d;\n}\nexport function main(): void {\n  print(`${f(0)}`);\n}\n",
    )]);
    match err {
        Err(RunError::Trap(t)) => {
            assert_eq!(t.rule, TrapKind::DivisionByZero);
            assert_eq!(t.pos.line, 2);
        }
        other => panic!("expected a division-by-zero trap, got {other:?}"),
    }
}
