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

/// Asserts both tiers trap with [`TrapKind::StrRange`] at `line` and
/// with **identical** kind/message/position (stdlib.md §8: the four Q21
/// trap paths report identically across tiers; the message text itself
/// comes from the one shared runtime, so equality here pins that no
/// tier adds its own wording).
fn assert_str_range_trap_identical(src: &str, line: u32) {
    let files = [SourceFile::new("test.ts", src)];
    let mut reports = Vec::new();
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::StrRange, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, line, "{tier}");
                reports.push((t.rule, t.message, t.pos));
            }
            other => panic!("{tier}: expected a StrRange trap, got {other:?}"),
        }
    }
    assert_eq!(reports[0], reports[1], "tiers disagree on the trap report");
}

#[test]
fn string_char_code_at_out_of_range_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  const s: string = \"abc\";\n  print(`${s.charCodeAt(3)}`);\n}\n",
        3,
    );
}

#[test]
fn string_repeat_negative_count_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  print(\"ab\".repeat(-1));\n}\n",
        2,
    );
}

#[test]
fn string_split_empty_separator_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  const parts: string[] = \"ab\".split(\"\");\n  print(`${parts.length}`);\n}\n",
        2,
    );
}

#[test]
fn string_replace_all_empty_pattern_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  print(\"ab\".replaceAll(\"\", \"x\"));\n}\n",
        2,
    );
}

#[test]
fn string_empty_pad_that_must_fill_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  print(\"ab\".padEnd(5, \"\"));\n}\n",
        2,
    );
}

#[test]
fn string_methods_match_across_tiers_without_a_golden() {
    // The committed a43 golden pins the full battery; this pins
    // cross-tier agreement for a compact slice with computed (non-
    // literal) receivers and arguments.
    assert_tiers_agree(
        "function part(xs: string[], i: i32): string {\n  return xs[i];\n}\nexport function main(): void {\n  const s: string = \"a\" + \"b,cb\";\n  const ps: string[] = s.split(\",\");\n  print(`${ps.length} ${part(ps, 0)} ${part(ps, 1)}`);\n  print(`${s.indexOf(part(ps, 1))} ${s.lastIndexOf(\"b\")} ${s.includes(\"b,\")}`);\n  print(s.toUpperCase().padStart(s.length + 2, \"_\").replaceAll(\"B\", \"x\"));\n}\n",
    );
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
fn date_now_reads_the_pinned_context_clock_in_the_ship_tier() {
    // stdlib.md §3: `Date.now()` is Context-owned and pinnable — the
    // ship-tier half of the both-tier pinned-clock check. The dev-tier
    // half is `jit.rs` (unit test
    // `date_now_reads_the_pinned_context_clock_in_the_dev_tier`): the
    // same program, the same pinned ms, and the same expected bytes.
    // Two tests rather than one because the dev tier's pinnable Context
    // is reachable only inside the crate; the shared expected bytes are
    // the cross-tier assertion.
    //
    // `run_c_aot` links the standing harness entry, which never pins
    // the clock, so this test drives the same pipeline itself with the
    // one difference: an entry derived from `AOT_ENTRY_C` that calls
    // `sub_rt_ctx_set_now(ctx, PINNED_MS)` before any program code
    // runs. The harness's own entry is untouched.
    use std::path::PathBuf;
    use std::process::Command;
    use subscript_codegen::{emit_c, runtime_staticlib_path, AOT_ENTRY_C};
    use subscript_compiler::check_program;

    const PINNED_MS: i64 = 1_592_224_496_789;
    const PROGRAM: &str = "export function main(): void {\n  const t: i64 = Date.now();\n  print(`${t}`);\n  print(new Date(Date.now()).toISOString());\n}\n";
    const EXPECTED: &[u8] = b"1592224496789\n2020-06-15T12:34:56.789Z\n";

    let hir = check_program(&[SourceFile::new("test.ts", PROGRAM)]).expect("checks clean");
    let program = emit_c(&hir).expect("ship C emission");
    let staticlib = runtime_staticlib_path().expect("runtime staticlib");

    let decl_anchor = "extern void ss_init(void *ctx);";
    let call_anchor = "    ss_init(ctx);";
    assert!(
        AOT_ENTRY_C.contains(decl_anchor) && AOT_ENTRY_C.contains(call_anchor),
        "AOT_ENTRY_C anchors moved; update this test's entry derivation"
    );
    let entry = AOT_ENTRY_C
        .replace(
            decl_anchor,
            "extern void sub_rt_ctx_set_now(void *ctx, int64_t ms);\nextern void ss_init(void *ctx);",
        )
        .replace(
            call_anchor,
            &format!("    sub_rt_ctx_set_now(ctx, {PINNED_MS});\n    ss_init(ctx);"),
        );

    // Temp dir removed on every exit path, including assertion panics.
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = std::env::temp_dir().join(format!(
        "subscript-cemit-pinned-now-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let _cleanup = Cleanup(dir.clone());

    let src_path = dir.join("program.c");
    let entry_path = dir.join("entry.c");
    let exe_path = dir.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&src_path, program.source.as_bytes()).expect("write program.c");
    std::fs::write(&entry_path, entry.as_bytes()).expect("write entry.c");

    // Same compile line as `run_c_aot` (§11): clang, -std=c11 -O2
    // -fwrapv -ffp-contract=off, interop header/impl, runtime staticlib.
    let interop = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop");
    let cc = std::env::var_os("CC").unwrap_or_else(|| "clang".into());
    #[cfg(all(windows, target_env = "msvc"))]
    let system_libs: &[&str] = &["-lkernel32", "-lntdll", "-luserenv", "-lws2_32", "-ldbghelp"];
    #[cfg(not(all(windows, target_env = "msvc")))]
    let system_libs: &[&str] = &[];
    let compile = Command::new(&cc)
        .arg("-std=c11")
        .arg("-O2")
        .arg("-fwrapv")
        .arg("-ffp-contract=off")
        .arg("-I")
        .arg(&interop)
        .arg(&src_path)
        .arg(&entry_path)
        .arg(interop.join("interop.c"))
        .arg(&staticlib)
        .args(system_libs)
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("run the C compiler (clang; set $CC)");
    assert!(
        compile.status.success(),
        "compiling/linking the emitted C failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&exe_path).output().expect("run linked program");
    assert!(
        run.status.success(),
        "linked program exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        run.stdout,
        EXPECTED,
        "ship tier printed {:?} for pinned ms {PINNED_MS}",
        String::from_utf8_lossy(&run.stdout)
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

