//! Ship-tier C-emission checks (`specs/blocks/compiler.md` §11).
//!
//! The full run-set differential (dev-JIT ≡ ship-C-AOT ≡ golden) lives
//! in `golden.rs`. This file pins ship-tier properties the committed
//! goldens do not exercise: the a22 checksum entry through the ship
//! path, reachable traps reported with kind and position (the trap
//! model), and — the P4.3 phase-review regressions — cross-tier
//! byte-equality (dev-JIT ≡ ship-C-AOT) for a mutating value method
//! (C1), non-i32 lambda captures (C2), and `collect()` interacting with
//! live handles (M1). Cross-tier byte-equality is the real invariant, so
//! these need no committed golden.

use subscript_codegen::{run_c_aot, run_jit, RunError};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

/// Asserts the dev-JIT and ship-C-AOT tiers print identical bytes for an
/// inline single-file program (the cross-tier invariant, §11).
fn assert_tiers_agree(src: &str) {
    let files = [SourceFile::new("test.ts", src)];
    let jit = run_jit(&files).expect("dev-JIT run");
    let ship = run_c_aot(&files).expect("ship-C-AOT run");
    assert_eq!(
        jit,
        ship,
        "tier mismatch:\n  dev-JIT   = {:?}\n  ship-C-AOT = {:?}",
        String::from_utf8_lossy(&jit),
        String::from_utf8_lossy(&ship)
    );
}

// ----- P4.3 phase-review regressions (dev-JIT ≡ ship-C-AOT) -----

#[test]
fn c1_mutating_value_method_persists_like_the_jit() {
    // A value method that mutates `this` must mutate the receiver (C2);
    // a non-mutating call on a copy must be unaffected.
    assert_tiers_agree(
        "@value\nclass V { x: i32; constructor(x: i32) { this.x = x; } bump(): void { this.x += 100; } }\nexport function main(): void {\n  const v: V = new V(1);\n  v.bump();\n  const c: V = v;\n  c.bump();\n  print(`${v.x},${c.x}`);\n}\n",
    );
}

#[test]
fn c2_capturing_lambda_over_f32_matches_the_jit() {
    assert_tiers_agree(
        "export function main(): void {\n  const offset: f32 = 0.5;\n  const add: (value: f32) => f32 = (value: f32): f32 => value + offset;\n  print(`${add(8.0)}`);\n}\n",
    );
}

#[test]
fn c2_capturing_lambda_over_i64_matches_the_jit() {
    assert_tiers_agree(
        "export function main(): void {\n  const base: i64 = 10000000000;\n  const add: (value: i64) => i64 = (value: i64): i64 => value + base;\n  print(`${add(1)}`);\n}\n",
    );
}

#[test]
fn c2_capturing_lambda_over_i32_matches_the_jit() {
    assert_tiers_agree(
        "export function main(): void {\n  const offset: i32 = 5;\n  const add: (value: i32) => i32 = (value: i32): i32 => value + offset;\n  print(`${add(7)}`);\n}\n",
    );
}

#[test]
fn m1_collect_then_delete_live_handle_matches_the_jit() {
    // The handle is live across collect(), so a single unsafeDelete must
    // succeed on both tiers (no spurious double-delete trap).
    assert_tiers_agree(
        "class C { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const a: C = new C(1);\n  collect();\n  unsafeDelete(a);\n  print(\"ok\");\n}\n",
    );
}

#[test]
fn m1_collect_then_use_live_handle_matches_the_jit() {
    assert_tiers_agree(
        "class C { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const a: C = new C(7);\n  collect();\n  print(`${a.x}`);\n  unsafeDelete(a);\n}\n",
    );
}

#[test]
fn m1_collect_keeps_references_inside_a_fixed_array_local_alive() {
    // The Box references live only inside a `FixedArray` local; the
    // shadow frame must root the aggregate's interior so collect() does
    // not mark them dead (which would synthesize a double-delete trap on
    // the following single unsafeDelete of a live handle).
    assert_tiers_agree(
        "class Box { value: i32; constructor(v: i32) { this.value = v; } }\nexport function main(): void {\n  const arr: FixedArray<Box, 2> = [new Box(10), new Box(20)];\n  collect();\n  print(`${arr[0].value},${arr[1].value}`);\n  unsafeDelete(arr[0]);\n  print(`ok`);\n}\n",
    );
}

#[test]
fn m1_collect_keeps_references_inside_a_fixed_array_param_alive() {
    // The CLIF path roots a managed-interior aggregate *parameter* by
    // copying it into the callee's shadow frame; the C tier must too, so
    // collect() inside the callee does not free the references.
    assert_tiers_agree(
        "class Box { value: i32; constructor(v: i32) { this.value = v; } }\nfunction probe(boxes: FixedArray<Box, 2>): i32 {\n  collect();\n  return boxes[0].value + boxes[1].value;\n}\nexport function main(): void {\n  const arr: FixedArray<Box, 2> = [new Box(3), new Box(4)];\n  print(`${probe(arr)}`);\n  unsafeDelete(arr[0]);\n  unsafeDelete(arr[1]);\n}\n",
    );
}

#[test]
fn date_intrinsics_match_across_tiers() {
    // stdlib.md §3: construction, accessors, carries, toISOString — the
    // committed a42 golden pins the full battery; this pins cross-tier
    // agreement for a compact slice without a golden.
    assert_tiers_agree(
        "export function main(): void {\n  const d: Date = new Date(Date.UTC(1999, 11, 31, 23, 59, 59, 999));\n  print(`${d.getTime()},${d.getUTCFullYear()},${d.getUTCDay()}`);\n  print(d.toISOString());\n  print(new Date(-1).toISOString());\n}\n",
    );
}

#[test]
fn ship_c_aot_reports_a_date_range_trap_with_its_position() {
    // Q20: out-of-range times trap — there is no Invalid-Date value.
    let files = [SourceFile::new(
        "test.ts",
        "export function main(): void {\n  const d: Date = new Date(8640000000000001);\n  print(d.toISOString());\n}\n",
    )];
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::DateRange, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, 2, "{tier}");
            }
            other => panic!("{tier}: expected a DateRange trap, got {other:?}"),
        }
    }
}

#[test]
fn ship_c_aot_reports_a_to_iso_year_range_trap() {
    // toISOString requires years 0000–9999 (stdlib.md §3); the TimeClip
    // maximum is a valid time but not printable.
    let files = [SourceFile::new(
        "test.ts",
        "export function main(): void {\n  const d: Date = new Date(8640000000000000);\n  print(d.toISOString());\n}\n",
    )];
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::DateRange, "{tier}");
                assert_eq!(t.pos.line, 3, "{tier}");
            }
            other => panic!("{tier}: expected a DateRange trap, got {other:?}"),
        }
    }
}

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

