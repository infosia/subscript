//! Ship-tier C-emission checks (`specs/blocks/compiler.md` §11).
//!
//! The full run-set differential (dev-JIT ≡ ship-C-AOT ≡ golden) lives
//! in `golden.rs`. This file pins ship-tier properties the committed
//! goldens do not exercise: the a22 checksum entry through the ship
//! path, reachable traps reported with kind and position (the trap
//! model), and — the P4.3 phase-review regressions — cross-tier
//! byte-equality (dev-JIT ≡ ship-C-AOT) for a mutating value method
//! (C1), non-i32 lambda captures (C2), and `Context.collect()` interacting with
//! live handles (M1). Cross-tier byte-equality is the real invariant, so
//! these need no committed golden.

#[path = "support/trap_corpus.rs"]
mod trap_corpus;
// The fixture is excluded on windows-msvc (compiler.md §11c), and the two
// interop trap probes are not run there, so this module and its symbols are
// gated out under the same predicate.
#[cfg(not(all(windows, target_env = "msvc")))]
#[path = "support/native_fixture.rs"]
mod native_fixture;

use subscript_codegen::{
    run_c_aot, run_c_aot_with_alloc_failure,
    run_c_aot_with_freed_handle_diagnostics_and_native_libraries, run_c_aot_with_native_libraries,
    run_jit, run_jit_with_alloc_failure,
    run_jit_with_freed_handle_diagnostics_and_native_libraries, run_jit_with_memory_accounting,
    run_jit_with_native_libraries, RunError, TrapReport,
};
// The MSVC branch uses `cc::windows_registry` and its own system library
// list, so these symbols have no use.
#[cfg(not(all(windows, target_env = "msvc")))]
use subscript_codegen::{host_c_compiler, runtime_system_libraries};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

type TrapOutcome = ((TrapKind, String, subscript_compiler::Pos), Vec<u8>);

fn trap_outcome(report: TrapReport) -> TrapOutcome {
    ((report.rule, report.message, report.pos), report.stdout)
}

fn render_run(result: &Result<Vec<u8>, RunError>) -> String {
    match result {
        Ok(stdout) => format!("Complete(stdout={:?})", String::from_utf8_lossy(stdout)),
        Err(RunError::Trap(report)) => format!(
            "Trap(kind={}, message={:?}, position={}, stdout={:?})",
            report.rule,
            report.message,
            report.pos,
            String::from_utf8_lossy(&report.stdout)
        ),
        Err(other) => format!("Error({other})"),
    }
}

fn trap_expectation(id: &str) -> (TrapKind, u32) {
    match id {
        "t01-json-result-value" => (TrapKind::JsonResultValue, 9),
        "t02-statements-after-fault" => (TrapKind::IndexOutOfBounds, 10),
        "t03-loop-stops-at-fault" => (TrapKind::IndexOutOfBounds, 13),
        "t04-call-after-fault" => (TrapKind::IndexOutOfBounds, 15),
        "t05-foreach-callback-fault" => (TrapKind::IndexOutOfBounds, 13),
        "t06-push-after-fault" => (TrapKind::IndexOutOfBounds, 11),
        "t07-p19-compound-assign-ordering" => (TrapKind::IndexOutOfBounds, 10),
        "t08-div-zero-expression"
        | "t09-rem-zero-expression"
        | "t10-div-zero-loop-condition"
        | "t11-rem-zero-loop-condition"
        | "t12-div-zero-array-element"
        | "t13-rem-zero-array-element" => (TrapKind::DivisionByZero, 10),
        "t14-div-zero-call-divisor" | "t15-rem-zero-call-divisor" => (TrapKind::DivisionByZero, 14),
        "t16-array-write-oob" => (TrapKind::IndexOutOfBounds, 11),
        "t17-fixed-array-read-oob" | "t18-fixed-array-write-oob" => (TrapKind::IndexOutOfBounds, 8),
        "t19-array-compound-second-write-oob" => (TrapKind::IndexOutOfBounds, 16),
        "t20-narrow-null" => (TrapKind::NullNarrowing, 20),
        "t21-narrow-class-mismatch" => (TrapKind::ClassMismatch, 27),
        "t22-double-delete-q6" => (TrapKind::DoubleDelete, 19),
        "t23-use-after-delete-q6" => (TrapKind::UseAfterDelete, 23),
        "t24-stale-coroutine-reload" => (TrapKind::StaleCoroutine, 19),
        "t25-allocation-sites-before-second-template-fault" => (TrapKind::DivisionByZero, 21),
        "t26-allocation-failure-new" => (TrapKind::AllocationFailure, 17),
        "t27-dynamic-value-field-write-oob" => (TrapKind::IndexOutOfBounds, 25),
        "t28-allocation-failure-array-literal" => (TrapKind::AllocationFailure, 9),
        "t29-allocation-failure-push-grow" => (TrapKind::AllocationFailure, 10),
        "t30-allocation-failure-string-concat" => (TrapKind::AllocationFailure, 11),
        "t31-allocation-failure-template" => (TrapKind::AllocationFailure, 10),
        "t32-allocation-failure-generator-frame" => (TrapKind::AllocationFailure, 7),
        "t33-allocation-failure-json-raw-new" => (TrapKind::AllocationFailure, 18),
        "t35-allocation-failure-map-new" | "t36-allocation-failure-set-new" => {
            (TrapKind::AllocationFailure, 9)
        }
        "t37-allocation-failure-map-grow" | "t38-allocation-failure-set-grow" => {
            (TrapKind::AllocationFailure, 10)
        }
        "t39-regex-budget" => (TrapKind::RegexBudget, 9),
        "t40-regex-invalid-pattern"
        | "t41-regex-unsupported-flag"
        | "t42-regex-duplicate-flag"
        | "t43-regex-mutually-exclusive-flags"
        | "t44-regex-replace-all-without-global"
        | "t45-regex-sticky-last-index" => (TrapKind::Regex, 8),
        "t46-callback-userdata-freed" => (TrapKind::CallbackUserdataFreed, 31),
        "t47-unreachable-reached" => (TrapKind::UnreachableReached, 10),
        "t48-wire-enum-unknown-value" => (TrapKind::WireEnumUnknownValue, 10),
        "t49-wire-enum-struct-unknown-member" => (TrapKind::WireEnumUnknownValue, 12),
        "t50-wire-entry-unknown-value" => (TrapKind::WireEnumUnknownValue, 8),
        "t51-bytes-into-range" => (TrapKind::IndexOutOfBounds, 18),
        other => panic!("{other}: trap corpus entry has no exact expectation"),
    }
}

fn regex_error_message(id: &str) -> Option<&'static str> {
    Some(match id {
        "t40-regex-invalid-pattern" => {
            "invalid regular expression: Unbalanced parenthesis"
        }
        "t41-regex-unsupported-flag" => {
            "unsupported regular-expression flag `q`; supported flags are d, g, i, m, s, u, v"
        }
        "t42-regex-duplicate-flag" => "duplicate regular-expression flag `g`",
        "t43-regex-mutually-exclusive-flags" => {
            "regular-expression flags `u` and `v` are mutually exclusive"
        }
        "t44-regex-replace-all-without-global" => {
            "string.replaceAll with a RegExp requires the `g` flag"
        }
        "t45-regex-sticky-last-index" => {
            "sticky regular-expression flag `y` requires mutable `RegExp.lastIndex`, which is not in the language"
        }
        _ => return None,
    })
}

fn allocation_failure_count(id: &str) -> Option<u64> {
    match id {
        "t26-allocation-failure-new" | "t28-allocation-failure-array-literal" => Some(2),
        "t29-allocation-failure-push-grow" => Some(3),
        "t30-allocation-failure-string-concat" => Some(4),
        "t31-allocation-failure-template" => Some(5),
        "t32-allocation-failure-generator-frame" => Some(2),
        "t33-allocation-failure-json-raw-new" => Some(5),
        "t35-allocation-failure-map-new" | "t36-allocation-failure-set-new" => Some(2),
        "t37-allocation-failure-map-grow" | "t38-allocation-failure-set-grow" => Some(3),
        _ => None,
    }
}

fn assert_trap_outcomes_identical(context: &str, outcomes: &[TrapOutcome]) {
    assert_eq!(
        outcomes.len(),
        2,
        "{context}: expected one trap outcome from each tier"
    );
    assert_eq!(
        outcomes[0],
        outcomes[1],
        "{context}: tiers disagree on (trap tuple, stdout)\n  dev-JIT stdout   = {:?}\n  \
         ship-C-AOT stdout = {:?}",
        String::from_utf8_lossy(&outcomes[0].1),
        String::from_utf8_lossy(&outcomes[1].1)
    );
}

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

/// Asserts both tiers print exactly `expected`. Used where cross-tier
/// equality alone cannot see the fault — a wrong result both tiers would
/// share (aliasing, formatting).
fn assert_tiers_print(src: &str, expected: &str) {
    let files = [SourceFile::new("test.ts", src)];
    for (tier, out) in [
        ("dev-JIT", run_jit(&files).expect("dev-JIT run")),
        ("ship-C-AOT", run_c_aot(&files).expect("ship-C-AOT run")),
    ] {
        assert_eq!(
            String::from_utf8_lossy(&out),
            expected,
            "{tier} printed unexpected bytes"
        );
    }
}

#[test]
fn narrow_integer_operations_wrap_at_the_declared_width_on_both_tiers() {
    assert_tiers_print(
        "export function main(): void {\n\
           const smin: i8 = -128;\n\
           const negOne: i8 = -1;\n\
           const sa: i8 = 100;\n\
           const sb: i8 = 30;\n\
           print(`${sa + sb},${sa - sb},${sa * sb},${smin / negOne},${smin % negOne}`);\n\
           const ua: u8 = 250;\n\
           const ub: u8 = 3;\n\
           print(`${ua + ub},${ua - ub},${ua * ub},${ua / ub},${ua % ub}`);\n\
           const wide: i16 = 30000;\n\
           const three: i16 = 3;\n\
           const uwide: u16 = 65000;\n\
           const thousand: u16 = 1000;\n\
           print(`${wide * three},${uwide + thousand}`);\n\
           const bits: i8 = -2;\n\
           const one: i8 = 1;\n\
           print(`${~bits},${bits >> one},${bits >>> one}`);\n\
           let compound: i8 = 120;\n\
           compound += 10;\n\
           compound *= 2;\n\
           const two: i8 = 2;\n\
           compound /= two;\n\
           print(`${compound}`);\n\
         }\n",
        "-126,70,-72,-128,0\n253,247,238,83,1\n24464,464\n1,-1,127\n2\n",
    );
}

#[test]
fn float_to_narrow_int_casts_saturate_to_the_narrow_range_on_both_tiers() {
    // The corpus only casts in-range floats to narrow ints, so overflow
    // saturation could regress silently. A float that overflows a narrow
    // integer must saturate to that width's own range (not wrap, and not
    // to the 32-bit range), and NaN must map to 0 — identically on the
    // dev-JIT and ship-C-AOT tiers, for signed and unsigned targets from
    // both f32 and f64 sources.
    assert_tiers_print(
        "export function main(): void {\n\
           const sHi: i8 = (300.0 as f32) as i8;\n\
           const sLo: i8 = (-200.0 as f32) as i8;\n\
           const uHi: u8 = (300.0 as f64) as u8;\n\
           const uLo: u8 = (-200.0 as f64) as u8;\n\
           const shHi: i16 = (70000.0 as f32) as i16;\n\
           const shLo: i16 = (-70000.0 as f64) as i16;\n\
           const uwHi: u16 = (70000.0 as f32) as u16;\n\
           const uwLo: u16 = (-5.0 as f64) as u16;\n\
           const nanS: i16 = (Math.sqrt(-1) as f32) as i16;\n\
           const nanU: u8 = (Math.sqrt(-1) as f64) as u8;\n\
           print(`${sHi},${sLo},${uHi},${uLo},${shHi},${shLo},${uwHi},${uwLo},${nanS},${nanU}`);\n\
         }\n",
        "127,-128,255,0,32767,-32768,65535,0,0,0\n",
    );
}

// ----- P4.3 phase-review regressions (dev-JIT ≡ ship-C-AOT) -----

#[test]
fn c1_mutating_value_method_persists_like_the_jit() {
    // A value method that mutates `this` must mutate the receiver (C2);
    // a non-mutating call on a copy must be unaffected.
    assert_tiers_agree(
        "@CStruct\nclass V { x: i32; constructor(x: i32) { this.x = x; } bump(): void { this.x += 100; } }\nexport function main(): void {\n  const v: V = new V(1);\n  v.bump();\n  const c: V = v;\n  c.bump();\n  print(`${v.x},${c.x}`);\n}\n",
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
    // The handle is live across Context.collect(), so a single Context.free must
    // succeed on both tiers (no spurious double-delete trap).
    assert_tiers_agree(
        "class C { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const a: C = new C(1);\n  Context.collect();\n  Context.free(a);\n  print(\"ok\");\n}\n",
    );
}

#[test]
fn m1_collect_then_use_live_handle_matches_the_jit() {
    assert_tiers_agree(
        "class C { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const a: C = new C(7);\n  Context.collect();\n  print(`${a.x}`);\n  Context.free(a);\n}\n",
    );
}

#[test]
fn m1_collect_keeps_references_inside_a_fixed_array_local_alive() {
    // The Box references live only inside a `FixedArray` local; the
    // shadow frame must root the aggregate's interior so Context.collect() does
    // not mark them dead (which would synthesize a double-delete trap on
    // the following single Context.free of a live handle).
    assert_tiers_agree(
        "class Box { value: i32; constructor(v: i32) { this.value = v; } }\nexport function main(): void {\n  const arr: FixedArray<Box, 2> = [new Box(10), new Box(20)];\n  Context.collect();\n  print(`${arr[0].value},${arr[1].value}`);\n  Context.free(arr[0]);\n  print(`ok`);\n}\n",
    );
}

#[test]
fn m1_collect_keeps_references_inside_a_fixed_array_param_alive() {
    // The CLIF path roots a managed-interior aggregate *parameter* by
    // copying it into the callee's shadow frame; the C tier must too, so
    // Context.collect() inside the callee does not free the references.
    assert_tiers_agree(
        "class Box { value: i32; constructor(v: i32) { this.value = v; } }\nfunction probe(boxes: FixedArray<Box, 2>): i32 {\n  Context.collect();\n  return boxes[0].value + boxes[1].value;\n}\nexport function main(): void {\n  const arr: FixedArray<Box, 2> = [new Box(3), new Box(4)];\n  print(`${probe(arr)}`);\n  Context.free(arr[0]);\n  Context.free(arr[1]);\n}\n",
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
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::DateRange, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, 2, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected a DateRange trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("Date constructor range trap", &outcomes);
}

#[test]
fn ship_c_aot_reports_a_to_iso_year_range_trap() {
    // toISOString requires years 0000–9999 (stdlib.md §3); the TimeClip
    // maximum is a valid time but not printable.
    let files = [SourceFile::new(
        "test.ts",
        "export function main(): void {\n  const d: Date = new Date(8640000000000000);\n  print(d.toISOString());\n}\n",
    )];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::DateRange, "{tier}");
                assert_eq!(t.pos.line, 3, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected a DateRange trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("Date.toISOString year range trap", &outcomes);
}

/// Asserts both tiers trap with [`TrapKind::StrRange`] at `line` and
/// with **identical** kind/message/position and pre-trap stdout
/// (stdlib.md §8: the four Q21 trap paths report identically across
/// tiers; the message text itself comes from the one shared runtime, so
/// equality here pins that no tier adds its own wording).
fn assert_str_range_trap_identical(src: &str, line: u32) {
    let files = [SourceFile::new("test.ts", src)];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::StrRange, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, line, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected a StrRange trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("String range trap", &outcomes);
}

#[test]
fn string_char_code_at_out_of_range_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  const s: string = \"abc\";\n  print(`${s.charCodeAt(3)}`);\n}\n",
        3,
    );
}

#[test]
fn string_char_at_off_utf8_boundary_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  print(\"é\".charAt(1));\n}\n",
        2,
    );
}

#[test]
fn string_code_point_at_off_utf8_boundary_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  print(`${\"é\".codePointAt(1)}`);\n}\n",
        2,
    );
}

#[test]
fn string_code_point_at_out_of_range_traps_identically() {
    assert_str_range_trap_identical(
        "export function main(): void {\n  print(`${\"a\".codePointAt(1)}`);\n}\n",
        2,
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

/// Asserts a Q25/Q26 programmer-error range trap has the same
/// (kind/message/position tuple, pre-trap stdout) on the dev-JIT and
/// ship-C-AOT tiers.
fn assert_number_range_trap_identical(src: &str, line: u32) {
    let files = [SourceFile::new("test.ts", src)];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(report)) => {
                assert_eq!(report.rule, TrapKind::NumberRange, "{tier}");
                assert_eq!(report.pos.file, "test.ts", "{tier}");
                assert_eq!(report.pos.line, line, "{tier}");
                outcomes.push(trap_outcome(report));
            }
            other => panic!("{tier}: expected a NumberRange trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("Number range trap", &outcomes);
}

#[test]
fn parse_int_out_of_range_radix_traps_identically() {
    assert_number_range_trap_identical(
        "export function main(): void {\n  print(`${parseInt(\"10\", 1)}`);\n}\n",
        2,
    );
}

#[test]
fn to_fixed_out_of_range_digits_trap_identically() {
    assert_number_range_trap_identical(
        "export function main(): void {\n  print((1.0).toFixed(101));\n}\n",
        2,
    );
}

/// Asserts a P13 JSON trap has an identical tuple and pre-trap stdout on
/// both lowering tiers.
fn assert_json_trap_identical(src: &str, kind: TrapKind, line: u32) {
    let files = [SourceFile::new("test.ts", src)];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(report)) => {
                assert_eq!(report.rule, kind, "{tier}");
                assert_eq!(report.pos.file, "test.ts", "{tier}");
                assert_eq!(report.pos.line, line, "{tier}");
                outcomes.push(trap_outcome(report));
            }
            other => panic!("{tier}: expected a {kind} trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("JSON trap", &outcomes);
}

#[test]
fn json_stringify_nan_traps_identically() {
    assert_json_trap_identical(
        "export function main(): void {\n  print(JSON.stringify(NaN));\n}\n",
        TrapKind::JsonNumber,
        2,
    );
}

#[test]
fn json_stringify_infinity_traps_identically() {
    assert_json_trap_identical(
        "export function main(): void {\n  print(JSON.stringify(Number.POSITIVE_INFINITY));\n}\n",
        TrapKind::JsonNumber,
        2,
    );
}

#[test]
fn json_stringify_cyclic_reference_graph_traps_identically() {
    assert_json_trap_identical(
        "class Node {\n  next: Node | null;\n  constructor() { this.next = null; }\n}\nexport function main(): void {\n  const node: Node = new Node();\n  node.next = node;\n  print(JSON.stringify(node));\n}\n",
        TrapKind::JsonCycle,
        8,
    );
}

#[test]
fn trap_corpus_entries_match_dev_stdout_on_both_tiers() {
    let trap = trap_corpus::corpus_trap();
    let ids = trap_corpus::trap_ids(&trap);
    let expected_count = 51;
    assert_eq!(
        ids.len(),
        expected_count,
        "expected exactly {expected_count} active trap entries (t01–t33 and t35–t38 runnable \
         coverage + t34 unrepresentable-layout policy, t39–t45 regex coverage, t46 \
         callback-userdata coverage, t47 unreachable coverage, t48 wire-enum crossing, and \
         t49 wire-enum boundary-member coverage, t50 wire-entry coverage, and t51 R34 byte-range coverage), found {}",
        ids.len()
    );

    let mut failures = Vec::new();
    for id in ids {
        // A stale coroutine exists only after two runs and a hot reload.
        // `reload.rs` drives this paired corpus source through that
        // dev-tier-only mode; a shipped C binary has no body-swap mode.
        if matches!(
            id.as_str(),
            "t24-stale-coroutine-reload" | "t34-allocation-failure-unrepresentable-policy"
        ) {
            continue;
        }
        let files = trap_corpus::trap_sources(&trap, &id);
        // On windows-msvc the interop fixture is excluded, so the two
        // narrowing probes (t20/t21) that make real foreign calls cannot run
        // there (compiler.md §11c). Every non-interop trap still runs.
        #[cfg(all(windows, target_env = "msvc"))]
        if files.iter().any(|source| {
            source.source.contains("subDevice")
                || source.source.contains("subWireMode")
                || source.source.contains("subBindTone")
        }) {
            continue;
        }
        let expected = trap_corpus::trap_expected(&trap, &id);
        let expected_file = format!("{id}.ts");
        let (expected_kind, expected_line) = trap_expectation(&id);
        let freed_handle_diagnostic = matches!(
            id.as_str(),
            "t22-double-delete-q6" | "t23-use-after-delete-q6"
        );
        let callback_userdata_diagnostic = id == "t46-callback-userdata-freed";
        let (jit, ship) = if id == "t50-wire-entry-unknown-value" {
            #[cfg(not(all(windows, target_env = "msvc")))]
            let libraries = [native_fixture::library()];
            #[cfg(all(windows, target_env = "msvc"))]
            let libraries: [subscript_codegen::NativeLibrary; 0] = [];
            (
                trap_corpus::run_wire_entry_unknown_dev(&files, &libraries),
                trap_corpus::run_wire_entry_unknown_ship(&files, &libraries),
            )
        } else if let Some(n) = allocation_failure_count(&id) {
            (
                run_jit_with_alloc_failure(&files, n),
                run_c_aot_with_alloc_failure(&files, n),
            )
        } else if freed_handle_diagnostic {
            (
                run_jit_with_memory_accounting(&files, true).map(|(stdout, _)| stdout),
                run_c_aot(&files),
            )
        } else if callback_userdata_diagnostic {
            #[cfg(not(all(windows, target_env = "msvc")))]
            let libraries = [native_fixture::library()];
            #[cfg(all(windows, target_env = "msvc"))]
            let libraries: [subscript_codegen::NativeLibrary; 0] = [];
            (
                run_jit_with_freed_handle_diagnostics_and_native_libraries(&files, &libraries),
                run_c_aot_with_freed_handle_diagnostics_and_native_libraries(&files, &libraries),
            )
        } else {
            #[cfg(not(all(windows, target_env = "msvc")))]
            let libraries = [native_fixture::library()];
            // No interop trap runs on windows-msvc, so the remaining entries
            // need no native library.
            #[cfg(all(windows, target_env = "msvc"))]
            let libraries: [subscript_codegen::NativeLibrary; 0] = [];
            (
                run_jit_with_native_libraries(&files, &libraries),
                run_c_aot_with_native_libraries(&files, &libraries),
            )
        };

        match &jit {
            Err(RunError::Trap(report)) => {
                if report.rule != expected_kind
                    || report.pos.file != expected_file
                    || report.pos.line != expected_line
                {
                    failures.push(format!(
                        "{id}: dev-JIT trap differs from the corpus intent\n  expected kind={expected_kind}, \
                         position={expected_file}:{expected_line}\n  actual   {}",
                        render_run(&jit)
                    ));
                }
                if report.stdout != expected {
                    failures.push(format!(
                        "{id}: dev-JIT stdout differs from its JIT-generated .expected\n  dev-JIT = \
                         {:?}\n  expected = {:?}",
                        String::from_utf8_lossy(&report.stdout),
                        String::from_utf8_lossy(&expected)
                    ));
                }
                if let Some(message) = regex_error_message(&id) {
                    if report.message != message {
                        failures.push(format!(
                            "{id}: dev-JIT regex message differs\n  expected = {message:?}\n  \
                             actual   = {:?}",
                            report.message
                        ));
                    }
                }
                let freed_handle_message = match id.as_str() {
                    "t22-double-delete-q6" => Some("Context.free of an already-deleted allocation"),
                    "t23-use-after-delete-q6" => Some("use of a deleted allocation"),
                    "t46-callback-userdata-freed" => {
                        Some("callback userdata points to a freed allocation")
                    }
                    "t48-wire-enum-unknown-value" => {
                        Some("unknown wire value 12345 for CEnum alias `SubWireMode`")
                    }
                    "t49-wire-enum-struct-unknown-member" => {
                        Some("unknown wire value 12345 for CEnum alias `SubWireMode`")
                    }
                    "t50-wire-entry-unknown-value" => {
                        Some("unknown wire value 12345 for CEnum alias `SubWireMode`")
                    }
                    "t51-bytes-into-range" => {
                        Some("byte range at offset 5 with size 16 exceeds array length 20")
                    }
                    _ => None,
                };
                if let Some(message) = freed_handle_message {
                    if report.message != message {
                        failures.push(format!(
                            "{id}: dev-JIT freed-handle message differs\n  expected = \
                             {message:?}\n  actual   = {:?}",
                            report.message
                        ));
                    }
                }
                if matches!(
                    id.as_str(),
                    "t35-allocation-failure-map-new"
                        | "t36-allocation-failure-set-new"
                        | "t37-allocation-failure-map-grow"
                        | "t38-allocation-failure-set-grow"
                ) && report.message != "injected allocation failure"
                {
                    failures.push(format!(
                        "{id}: Context::trap must preserve the first, injected message; got {:?}",
                        report.message
                    ));
                }
            }
            _ => failures.push(format!(
                "{id}: dev-JIT did not produce the intended trap\n  {}",
                render_run(&jit)
            )),
        }

        println!(
            "{id}:\n  dev-JIT    {}\n  ship-C-AOT {}",
            render_run(&jit),
            render_run(&ship)
        );

        // Q6/§8.1b explicitly makes double-delete and use-after-delete
        // undefined in the releasing ship runtime. Execute that column
        // so its observed behavior stays visible, but contract only the
        // dev-JIT trap and golden rather than asserting tier agreement.
        if matches!(
            id.as_str(),
            "t22-double-delete-q6" | "t23-use-after-delete-q6"
        ) {
            continue;
        }

        match (&jit, &ship) {
            (Err(RunError::Trap(dev)), Err(RunError::Trap(c))) if dev == c => {}
            _ => failures.push(format!(
                "{id}: tiers disagree on (kind, message, position, pre-fault stdout)\n  dev-JIT    \
                 {}\n  ship-C-AOT {}",
                render_run(&jit),
                render_run(&ship)
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} trap-corpus failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn p20_review_accept_entries_reach_both_generators() {
    let accept = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
    let mut failures = Vec::new();
    for (id, expected) in [
        ("a74-p20-string-array-compound", b"as\n".as_slice()),
        ("a75-p20-array-compound-expression", b"17,17\n".as_slice()),
        ("a76-p20-dynamic-value-field-write", b"14,1\n".as_slice()),
    ] {
        let path = accept.join(format!("{id}.ts"));
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let files = [SourceFile::new(format!("{id}.ts"), source)];
        let jit = run_jit(&files);
        let ship = run_c_aot(&files);
        if !matches!(&jit, Ok(stdout) if stdout == expected) {
            failures.push(format!(
                "{id}: dev-JIT did not print {:?}: {}",
                String::from_utf8_lossy(expected),
                render_run(&jit)
            ));
        }
        if !matches!(&ship, Ok(stdout) if stdout == expected) {
            failures.push(format!(
                "{id}: ship-C-AOT did not print {:?}: {}",
                String::from_utf8_lossy(expected),
                render_run(&ship)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} P20 Red accept failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn out_of_range_320_byte_cstruct_store_stops_before_the_store() {
    // P19: the old ship-tier path let subscript_arr_at return its 256-byte
    // scratch sentinel after trapping, then copied this 320-byte value
    // into it. Under ASan that was a global-buffer-overflow. The store
    // must now be unreachable, in addition to stdout matching the dev
    // tier.
    let fields = std::iter::repeat("0")
        .take(80)
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "@CStruct\n\
         class Wide {{\n\
           words: FixedArray<i32, 80>;\n\
           constructor(words: FixedArray<i32, 80>) {{ this.words = words; }}\n\
         }}\n\
         export function main(): void {{\n\
           const values: Wide[] = [];\n\
           const wide: Wide = new Wide([{fields}]);\n\
           print(\"before\");\n\
           values[0] = wide;\n\
           print(\"after\");\n\
         }}\n"
    );
    let files = [SourceFile::new("test.ts", source)];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(report)) => {
                assert_eq!(report.rule, TrapKind::IndexOutOfBounds, "{tier}");
                assert_eq!(report.stdout, b"before\n", "{tier}");
                outcomes.push(trap_outcome(report));
            }
            other => {
                panic!("{tier}: expected an out-of-bounds trap, got {other:?}")
            }
        }
    }
    assert_trap_outcomes_identical("320-byte CStruct array store", &outcomes);
}

#[test]
fn failed_json_result_string_and_reference_payloads_trap_identically() {
    for (source, line) in [
        (
            "export function main(): void {\n  const failed: JsonResult<string> = JSON.parse<string>(\"nope\");\n  print(failed.value);\n}\n",
            3,
        ),
        (
            "class Box {\n  name: string;\n  constructor() { this.name = \"box\"; }\n}\nexport function main(): void {\n  const failed: JsonResult<Box> = JSON.parse<Box>(\"nope\");\n  print(failed.value.name);\n}\n",
            7,
        ),
        (
            "class Box {\n  name: string;\n  constructor() { this.name = \"box\"; }\n}\nfunction read(result: JsonResult<Box>): Box {\n  return result.value;\n}\nexport function main(): void {\n  const failed: JsonResult<Box> = JSON.parse<Box>(\"nope\");\n  print(read(failed).name);\n}\n",
            6,
        ),
    ] {
        assert_json_trap_identical(source, TrapKind::JsonResultValue, line);
    }
}

#[test]
fn binary32_bit_access_uses_the_declared_runtime_symbols() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "export function main(): void {\n  const bits: u32 = Math.f32ToBits(1);\n  print(`${Math.f32FromBits(bits)}`);\n}\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    assert!(c.contains("extern uint32_t subscript_rt_math_f32_to_bits(void* ctx, double x);"));
    assert!(c.contains("extern double subscript_rt_math_f32_from_bits(void* ctx, uint32_t bits);"));
    assert!(c.contains("subscript_rt_math_f32_to_bits(ctx, 1.0)"), "{c}");
    assert!(
        c.contains("subscript_rt_math_f32_from_bits(ctx, bits)"),
        "{c}"
    );
}

#[test]
fn aligned_value_class_emits_alignas_on_the_first_field() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "@CStruct({ align: 16 })\nclass Vec3f { x: f32; y: f32; z: f32; }\nexport function main(): void { const value: Vec3f = new Vec3f(); print(`${value.x}`); }\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    assert!(c.contains("    _Alignas(16) float x;"), "{c}");
}

#[test]
fn host_callable_export_emits_handle_and_scalar_parameters() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let files = [
        SourceFile::ambient(
            "state.generated.d.ts",
            "// @subscript-c-header include=\"state.h\"\n\
             interface HostState {\n\
             \x20 readonly __sub_handle_HostState: never;\n\
             }\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function adopt(state: HostState, tag: i32): void {\n\
             \x20 if (tag === 0) { print(`${state === state}`); }\n\
             }\n\
             export function main(): void {}\n",
        ),
    ];
    let hir = check_program(&files).expect("host export checks cleanly");
    let c = emit_c(&hir).expect("host export emits C").source;
    assert!(
        c.contains(
            "void subscript_export_adopt(subscript_rt_context* ctx, void* state, int32_t tag) { subscript_fn_adopt(ctx, state, tag); }"
        ),
        "parameterized host wrapper is missing:\n{c}"
    );
}

#[test]
fn wire_alias_entry_wrapper_validates_before_the_internal_call() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "type WireMode = CEnum<{ \"m0\": 16; \"m1\": 23; \"m2\": -7 }>;\n\
                  export function configure(mode: WireMode, tag: i32): void {}\n\
                  export function main(): void {}\n";
    let hir = check_program(&[SourceFile::new("wire-entry.ts", source)])
        .expect("wire entry checks cleanly");
    let program = emit_c(&hir).expect("wire entry emits C");
    let wrapper = program
        .source
        .split("void subscript_export_configure")
        .nth(1)
        .expect("wire entry wrapper");
    let validation = wrapper.find("mode == 16").expect("wire value validation");
    let trap = wrapper
        .find("subscript_rt_trap_wire_enum(ctx,")
        .expect("wire value trap");
    let call = wrapper
        .find("subscript_fn_configure(ctx, mode, tag);")
        .expect("internal entry call");
    assert!(
        validation < trap && trap < call,
        "wire entry wrapper does not validate before calling the entry:\n{wrapper}"
    );
    assert!(wrapper.contains("mode == 23") && wrapper.contains("mode == -7"));
    assert!(wrapper.contains("WireMode"));
    assert!(wrapper.contains("return;"));
    assert!(
        program
            .positions
            .iter()
            .any(|position| position.file == "wire-entry.ts" && position.line == 2),
        "wire entry trap position does not point at the parameter declaration"
    );
}

#[test]
fn parameterized_async_export_has_no_host_wrapper() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "async function later(tag: i32): Promise<void> {\n\
                  \x20 print(`${tag}`);\n\
                  }\n\
                  export function main(): void {}\n";
    let mut hir =
        check_program(&[SourceFile::new("test.ts", source)]).expect("async function checks");
    hir.functions
        .iter_mut()
        .find(|function| function.name == "later")
        .expect("later function")
        .exported = true;
    let c = emit_c(&hir).expect("async export emits C").source;
    assert!(c.contains("static void* subscript_fn_later(void* ctx, int32_t tag)"));
    assert!(
        !c.contains("subscript_export_later"),
        "parameterized async export gained a host wrapper:\n{c}"
    );
}

#[test]
fn generic_async_instance_has_no_host_wrapper() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "export async function go<T>(): Promise<void> {\n\
                  \x20 await Context.suspend();\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 await go<u32>();\n\
                  }\n";
    let hir =
        check_program(&[SourceFile::new("test.ts", source)]).expect("generic async program checks");
    let c = emit_c(&hir).expect("generic async program emits C").source;

    assert!(
        c.contains("subscript_export_main"),
        "main wrapper is missing:\n{c}"
    );
    assert!(
        !c.contains("subscript_export_go"),
        "generic instance gained a host wrapper:\n{c}"
    );
}

#[test]
fn acyclic_json_serializer_emits_no_tracking_operations() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "class Box {\n  value: i32;\n  constructor(value: i32) { this.value = value; }\n}\nexport function main(): void {\n  print(JSON.stringify(new Box(7)));\n}\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    assert!(c.contains("subscript_rt_json_begin(ctx,"), "{c}");
    assert!(!c.contains("subscript_rt_json_begin_tracked(ctx,"), "{c}");
    assert!(!c.contains("subscript_rt_json_visit(ctx,"), "{c}");
    assert!(!c.contains("subscript_rt_json_leave(ctx,"), "{c}");
}

#[test]
fn constructor_less_value_class_emits_field_initializer_store() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "@CStruct\nclass ValueField {\n  value: i32 = 37;\n}\nexport function main(): void {\n  const field: ValueField = new ValueField();\n  print(`${field.value}`);\n}\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    assert!(
        c.lines()
            .any(|line| line.contains(".value = 37;") && line.contains("_t")),
        "constructor-less value initializer store is missing:\n{c}"
    );
}

#[test]
fn constructor_less_reference_class_emits_field_initializer_store() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "class ReferenceField {\n  value: i32 = 41;\n}\nexport function main(): void {\n  const field: ReferenceField = new ReferenceField();\n  print(`${field.value}`);\n}\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    assert!(
        c.lines().any(|line| {
            line.contains("((Sub_0_ReferenceField*)") && line.contains(")->value = 41;")
        }),
        "constructor-less reference initializer store is missing:\n{c}"
    );
}

#[test]
fn string_literal_union_equality_emits_an_integer_compare() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "type Format = \"uint16\" | \"uint32\";\n\
                  function same(left: Format, right: Format): boolean {\n\
                    return left === right;\n\
                  }\n\
                  export function main(): void {\n\
                    const left: Format = \"uint16\";\n\
                    print(`${same(left, \"uint16\")}`);\n\
                  }\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    let comparison = c
        .lines()
        .find(|line| line.contains("return") && line.contains("left") && line.contains("right"))
        .unwrap_or_else(|| panic!("alias equality return is missing:\n{c}"));
    assert_eq!(comparison.trim(), "return (left == right);");
    assert!(
        !comparison.contains("subscript_rt_str_eq"),
        "Q32 equality called string comparison: {comparison}"
    );
}

#[test]
fn wire_enum_foreign_crossing_is_identity_with_unknown_return_trap() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let mirror = "// @subscript-c-header include=\"wire.h\"\n\
                  type WireMode = CEnum<{ \"m0\": 0x10; \"m1\": 23; \"m2\": -7 }>;\n\
                  declare function subWireTake(value: WireMode): i32;\n\
                  declare function subWireReturn(): WireMode;\n";
    let source = "export function main(): void {\n\
                    const returned: WireMode = subWireReturn();\n\
                    subWireTake(\"m2\");\n\
                    subWireTake(returned);\n\
                  }\n";
    let hir = check_program(&[
        SourceFile::ambient("wire.d.ts", mirror),
        SourceFile::new("test.ts", source),
    ])
    .expect("wire-enum crossing checks cleanly");
    let c = emit_c(&hir).expect("wire-enum crossing emits C").source;
    assert!(
        !c.contains("subscript_wire_values_0") && !c.contains("subscript_wire_alias0"),
        "wire-table storage survived the representation revision:\n{c}"
    );
    assert!(
        c.lines().any(|line| line.contains("subWireTake(-7)")),
        "member literal was not passed as its wire value:\n{c}"
    );
    assert!(
        c.contains("subWireReturn()")
            && c.contains("== 16")
            && c.contains("== 23")
            && c.contains("== -7"),
        "return crossing did not validate wire membership:\n{c}"
    );
    assert!(
        c.contains("subscript_rt_trap_wire_enum(ctx,") && c.contains("WireMode"),
        "return crossing lacks the shared dynamic trap path:\n{c}"
    );
    let main = c
        .split("void subscript_export_main(subscript_rt_context* ctx) {")
        .nth(1)
        .expect("main body");
    assert!(
        !main.contains("subscript_rt_str_lit(ctx,")
            && !main.contains("subscript_rt_str_eq(ctx,")
            && !main.contains("subscript_string_alias_0["),
        "wire crossing performed a string operation:\n{main}"
    );
}

#[test]
fn wire_enum_switch_formatting_and_boundary_member_read_use_wire_values() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let mirror = "// @subscript-c-header include=\"wire.h\"\n\
                  type WireMode = CEnum<{ \"m0\": 0x10; \"m1\": 23; \"m2\": -7 }>;\n\
                  declare class WireRecord {\n\
                    mode: WireMode;\n\
                    constructor(mode: WireMode);\n\
                  }\n\
                  declare function subWireFill(value: WireRecord | null): void;\n";
    let source = "export function main(): void {\n\
                    const record: WireRecord = new WireRecord(\"m2\");\n\
                    subWireFill(record);\n\
                    const mode: WireMode = record.mode;\n\
                    switch (mode) {\n\
                      case \"m0\": print(`${mode}`); break;\n\
                      case \"m1\": print(`${mode}`); break;\n\
                      case \"m2\": print(`${mode}`); break;\n\
                    }\n\
                  }\n";
    let hir = check_program(&[
        SourceFile::ambient("wire.d.ts", mirror),
        SourceFile::new("test.ts", source),
    ])
    .expect("wire boundary member checks cleanly");
    let c = emit_c(&hir).expect("wire boundary member emits C").source;
    for label in ["case 16:", "case 23:", "case -7:"] {
        assert!(
            c.contains(label),
            "missing wire-valued switch label `{label}`:\n{c}"
        );
    }
    assert!(
        c.contains("subscript_rt_trap_wire_enum(ctx,")
            && c.contains("WireMode")
            && c.contains("== 16")
            && c.contains("== 23")
            && c.contains("== -7"),
        "boundary member read lacks wire membership validation:\n{c}"
    );
    let main = c
        .split("static void subscript_fn_main(void* ctx) {")
        .nth(1)
        .expect("main body");
    assert!(
        main.contains("subscript_string_alias_0[")
            && !main.contains("subscript_rt_str_eq")
            && !main.contains("strcmp("),
        "wire formatting must resolve its string entry with integer lookup only:\n{main}"
    );
}

#[test]
fn absence_presence_test_emits_reserved_integer_compare() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "type Compare = \"never\" | \"less\";\n\
                  @Descriptor\n\
                  class Sampler { compare?: Compare; }\n\
                  function present(sampler: Sampler): boolean {\n\
                    return sampler.compare !== undefined;\n\
                  }\n\
                  export function main(): void {\n\
                    const sampler: Sampler = {};\n\
                    print(`${present(sampler)}`);\n\
                  }\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emits C").source;
    let comparison = c
        .lines()
        .find(|line| line.contains("compare") && line.contains("!= -1"))
        .unwrap_or_else(|| panic!("absence test is not an integer compare against -1:\n{c}"));
    assert!(
        !comparison.contains("subscript_string_alias"),
        "presence comparison consulted the Q32 formatting table: {comparison}"
    );
    assert!(
        c.lines().any(|line| line.contains("->compare = -1")),
        "omission did not store the reserved -1 discriminant:\n{c}"
    );
}

#[test]
fn a115_string_literal_union_switch_emits_integer_c_switches() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = include_str!("../../corpus/accept/a115-switch-literal-union.ts");
    let hir = check_program(&[SourceFile::new("a115-switch-literal-union.ts", source)])
        .expect("a115 checks cleanly");
    let c = emit_c(&hir).expect("a115 emits C").source;
    assert_eq!(
        c.matches("int32_t _disc =").count(),
        2,
        "a115 discriminants are not emitted as i32:\n{c}"
    );
    assert_eq!(
        c.matches("switch (_disc) {").count(),
        2,
        "a115 does not emit one C switch per source switch:\n{c}"
    );
    let labels = c
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            line.strip_prefix("case ")
                .and_then(|rest| rest.split_once(":"))
                .map(|(value, _)| value)
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, ["0", "1", "2", "1"]);
    assert!(
        !c.contains("if (_disc =="),
        "Q32 switch fell back to a comparison chain:\n{c}"
    );
    assert!(
        !c.contains("subscript_rt_str_eq(ctx,"),
        "Q32 switch called string comparison:\n{c}"
    );
}

#[test]
fn descriptor_nested_defaults_are_fresh_per_construction() {
    assert_tiers_print(
        "@Descriptor\n\
         class Child {\n\
           value?: i32 = 1;\n\
         }\n\
         @Descriptor\n\
         class Parent {\n\
           child?: Child = {};\n\
         }\n\
         export function main(): void {\n\
           const first: Parent = {};\n\
           const second: Parent = {};\n\
           first.child.value = 9;\n\
           print(`${first.child.value},${second.child.value},${first.child === second.child}`);\n\
         }\n",
        "9,1,false\n",
    );
}

fn provenance_fixture() -> subscript_compiler::hir::Module {
    use subscript_compiler::check_program;

    const ENGINE_MIRROR: &str = "\
// @subscript-c-header include=\"engine.h\"
// @subscript-c-descriptor function=\"engineUse\" parameter=\"engineRead\" aggregate=\"EngineItemView\" element=\"EngineItem\" const=true
// @subscript-c-descriptor function=\"engineUse\" parameter=\"engineWrite\" aggregate=\"EngineItemOut\" element=\"EngineItem\" const=false
// @subscript-c-string-view function=\"engineUse\" parameter=\"engineLabel\" aggregate=\"EngineStringView\"
// @subscript-c-callback typedef=\"EngineCallback\"
type EngineCallback = (engineMessage: string, engineUserdata1: object | null, engineUserdata2: object | null) => void;
declare class EngineItem {
  engineValue: u32;
  constructor(engineValue: u32);
}
declare class EngineSink {
  engineCallback: EngineCallback;
  engineUserdata1: object | null;
  engineUserdata2: object | null;
  constructor(engineCallback: EngineCallback, engineUserdata1: object | null, engineUserdata2: object | null);
}
declare function engineUse(engineRead: EngineItem[], engineWrite: EngineItem[], engineLabel: string, engineSink: EngineSink): void;
";
    const AUDIO_MIRROR: &str = "\
// @subscript-c-header include=\"audio.h\"
declare function audTick(audFrames: u32): void;
";
    const PROGRAM: &str = "\
export function main(): void {
  const engineRead: EngineItem[] = [new EngineItem(1)];
  const engineWrite: EngineItem[] = [new EngineItem(0)];
  const engineSink: EngineSink = new EngineSink(
    (engineMessage, engineUserdata1, engineUserdata2) => {},
    null,
    null,
  );
  engineUse(engineRead, engineWrite, \"label\", engineSink);
  audTick(64);
}
";
    check_program(&[
        SourceFile::ambient("engine.generated.d.ts", ENGINE_MIRROR),
        SourceFile::ambient("audio.generated.d.ts", AUDIO_MIRROR),
        SourceFile::new("test.ts", PROGRAM),
    ])
    .expect("provenance fixture checks")
}

#[test]
fn foreign_c_names_come_from_typed_mirror_provenance() {
    use subscript_codegen::emit_c;

    let c = emit_c(&provenance_fixture())
        .expect("provenance fixture emits")
        .source;
    let engine_include = c.find("#include \"engine.h\"").expect("engine include");
    let audio_include = c.find("#include \"audio.h\"").expect("audio include");
    assert!(
        engine_include < audio_include,
        "mirror ingestion order: {c}"
    );
    assert_eq!(c.matches("#include \"engine.h\"").count(), 1, "{c}");
    assert_eq!(c.matches("#include \"audio.h\"").count(), 1, "{c}");
    assert!(
        c.contains("typedef struct subscript_callback_string_view"),
        "{c}"
    );
    assert!(
        c.contains(
            "extern void subscript_rt_cb_trampoline(subscript_callback_string_view message, void* userdata1, void* userdata2);"
        ),
        "{c}"
    );
    assert!(c.contains("((EngineStringView){"), "{c}");
    assert!(c.contains("((EngineItemView){"), "{c}");
    assert!(c.contains("((EngineItemOut){ (EngineItem*)"), "{c}");
    assert!(
        c.contains("(EngineCallback)&subscript_rt_cb_trampoline"),
        "{c}"
    );
}

#[test]
fn missing_emission_site_provenance_is_an_internal_error_naming_the_site() {
    use subscript_codegen::emit_c;

    let mut missing_parameter = provenance_fixture();
    let parameter = missing_parameter
        .foreign_fns
        .iter_mut()
        .find(|function| function.name == "engineUse")
        .and_then(|function| {
            function
                .params
                .iter_mut()
                .find(|parameter| parameter.name == "engineLabel")
        })
        .expect("string parameter");
    parameter.foreign_provenance = None;
    let error = emit_c(&missing_parameter).expect_err("missing string provenance must fail");
    assert!(error.contains("internal error"), "{error}");
    assert!(error.contains("engineUse"), "{error}");
    assert!(error.contains("engineLabel"), "{error}");

    let mut missing_descriptor = provenance_fixture();
    let parameter = missing_descriptor
        .foreign_fns
        .iter_mut()
        .find(|function| function.name == "engineUse")
        .and_then(|function| {
            function
                .params
                .iter_mut()
                .find(|parameter| parameter.name == "engineWrite")
        })
        .expect("descriptor parameter");
    parameter.foreign_provenance = None;
    let error = emit_c(&missing_descriptor).expect_err("missing descriptor provenance must fail");
    assert!(error.contains("internal error"), "{error}");
    assert!(error.contains("engineUse"), "{error}");
    assert!(error.contains("engineWrite"), "{error}");

    let mut missing_callback = provenance_fixture();
    let field = missing_callback
        .classes
        .iter_mut()
        .find(|class| class.name == "EngineSink")
        .and_then(|class| {
            class
                .fields
                .iter_mut()
                .find(|field| field.name == "engineCallback")
        })
        .expect("callback field");
    field.foreign_provenance = None;
    let error = emit_c(&missing_callback).expect_err("missing callback provenance must fail");
    assert!(error.contains("internal error"), "{error}");
    assert!(error.contains("EngineSink"), "{error}");
    assert!(error.contains("engineCallback"), "{error}");

    let mut missing_mirror = provenance_fixture();
    missing_mirror.foreign_mirrors.clear();
    let error = emit_c(&missing_mirror).expect_err("missing mirror provenance must fail");
    assert!(error.contains("internal error"), "{error}");
    assert!(error.contains("foreign C preamble"), "{error}");
}

#[test]
fn to_string_out_of_range_radix_traps_identically() {
    assert_number_range_trap_identical(
        "export function main(): void {\n  print((1.0).toString(37));\n}\n",
        2,
    );
}

#[test]
fn to_exponential_out_of_range_digits_trap_identically() {
    assert_number_range_trap_identical(
        "export function main(): void {\n  print((1.0).toExponential(101));\n}\n",
        2,
    );
}

#[test]
fn to_precision_out_of_range_digits_trap_identically() {
    assert_number_range_trap_identical(
        "export function main(): void {\n  print((1.0).toPrecision(0));\n}\n",
        2,
    );
}

#[test]
fn array_trapping_map_callback_reports_identically_across_tiers() {
    // stdlib.md §9 gate: a callback that traps mid-`map` (an OOB index
    // inside the closure at v == 3) aborts the iteration in the shared
    // runtime; the standing post-call trap check surfaces it on both
    // tiers with an identical (kind/message/position tuple, stdout).
    let files = [SourceFile::new(
        "test.ts",
        "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const ys: i32[] = xs.map((v: i32): i32 => xs[v + 1]);\n  print(`${ys.length}`);\n}\n",
    )];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::IndexOutOfBounds, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, 3, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected an out-of-bounds trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("Array.map callback trap", &outcomes);
}

#[test]
fn array_empty_shift_reports_identically_across_tiers() {
    let files = [SourceFile::new(
        "test.ts",
        "export function main(): void {\n  const xs: i32[] = [];\n  print(`${xs.shift()}`);\n}\n",
    )];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::EmptyPop, "{tier}");
                assert_eq!(t.message, "shift() on an empty array", "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, 3, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected an empty-array trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("Array.shift trap", &outcomes);
}

#[test]
fn array_methods_match_across_tiers_without_a_golden() {
    // The committed a44/a45 goldens pin the full batteries; this pins
    // cross-tier agreement for a compact slice with computed receivers,
    // a function-typed local as the callback, and a capturing
    // comparator (C5: non-escaping, legal as a callback).
    assert_tiers_agree(
        "export function main(): void {\n  const xs: i32[] = [3, 1, 2];\n  const pred: (v: i32) => boolean = (v: i32): boolean => v > 1;\n  print(`${xs.filter(pred).length} ${xs.findIndex(pred)}`);\n  const pivot: i32 = 2;\n  xs.sort((a: i32, b: i32): i32 => (a === pivot ? -1 : a) - (b === pivot ? -1 : b));\n  print(xs.slice(0).concat(xs).join(\",\"));\n}\n",
    );
}

/// Asserts both tiers abort an `Array` callback method with the same
/// out-of-bounds trap tuple (kind, message, position) and pre-trap
/// stdout — stdlib.md §9's callback-trap rule, for the methods the `map`
/// test above does not cover.
fn assert_callback_trap_identical(src: &str, line: u32) {
    let files = [SourceFile::new("test.ts", src)];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::IndexOutOfBounds, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, line, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected an out-of-bounds trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("Array callback trap", &outcomes);
}

#[test]
fn array_trapping_callbacks_report_identically_across_tiers() {
    // The remaining seven closure methods (`map` has its own test): a
    // callback that indexes past the end aborts the iteration in the
    // shared runtime and surfaces the identical trap tuple and stdout
    // on both tiers. `sort`'s additional §9 guarantee — a comparator trap
    // leaves the receiver byte-identical — is not observable in-language
    // (the trap returns from the function), so it is pinned in the
    // shared runtime instead (`arrops.rs`, `callback_traps_abort_...`).
    const PROLOGUE: &str =
        "let sink: i32 = 0;\nexport function main(): void {\n  const xs: i32[] = [1, 2, 3];\n";
    for call in [
        "xs.forEach((v: i32): void => { sink = sink + xs[v + 1]; });",
        "sink = xs.filter((v: i32): boolean => xs[v + 1] > 0).length;",
        "sink = xs.reduce((acc: i32, v: i32): i32 => acc + xs[v + 1], 0);",
        "sink = xs.some((v: i32): boolean => xs[v + 1] > 5) ? 1 : 0;",
        "sink = xs.every((v: i32): boolean => xs[v + 1] > 0) ? 1 : 0;",
        "sink = xs.findIndex((v: i32): boolean => xs[v + 1] > 5);",
        "sink = xs.sort((a: i32, b: i32): i32 => xs[a + b] - 1).length;",
    ] {
        assert_callback_trap_identical(
            &format!("{PROLOGUE}  {call}\n  print(`${{sink}}`);\n}}\n"),
            4,
        );
    }
}

#[test]
fn array_callback_growth_during_iteration_is_defined_on_both_tiers() {
    // The callback pushes while the runtime iterates the receiver, well
    // past the initial capacity, so the storage moves; the runtime
    // re-resolves the element pointer per element (`arrops.rs`
    // `read_elem`). The result is defined and identical on both tiers.
    assert_tiers_print(
        "let seen: i32 = 0;\nexport function main(): void {\n  const xs: i32[] = [];\n  let i: i32 = 0;\n  while (i < 8) {\n    xs.push(i);\n    i = i + 1;\n  }\n  xs.forEach((v: i32): void => {\n    seen = seen + v;\n    xs.push(v + 100);\n  });\n  const doubled: i32[] = xs.map((v: i32): i32 => {\n    xs.push(v);\n    return v * 2;\n  });\n  print(`${seen} ${xs.length} ${doubled.length} ${doubled[0]} ${doubled[15]}`);\n}\n",
        "28 32 16 0 214\n",
    );
}

#[test]
fn map_set_corpus_entries_match_across_tiers_before_golden_capture() {
    let accept = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
    for id in [
        "a51-map",
        "a52-map-order",
        "a53-set",
        "a54-map-reference-key",
        "a55-map-set-foreach",
        "a56-map-aggregate-foreach",
        "a61-same-value-zero",
    ] {
        let path = accept.join(format!("{id}.ts"));
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let sources = [SourceFile::new(format!("{id}.ts"), source)];
        let jit = run_jit(&sources).unwrap_or_else(|e| panic!("{id}: dev-JIT run failed: {e}"));
        let ship =
            run_c_aot(&sources).unwrap_or_else(|e| panic!("{id}: ship-C-AOT run failed: {e}"));
        assert_eq!(
            jit,
            ship,
            "{id}: dev-JIT output {:?} != ship-C-AOT output {:?}",
            String::from_utf8_lossy(&jit),
            String::from_utf8_lossy(&ship)
        );
    }
}

#[test]
fn map_and_set_trapping_foreach_callbacks_report_identically() {
    for src in [
        "export function main(): void {\n  const probe: i32[] = [7];\n  const map: Map<i32, i32> = new Map<i32, i32>();\n  map.set(1, 1);\n  map.forEach((value: i32, key: i32): void => { print(`${probe[value + key]}`); });\n}\n",
        "export function main(): void {\n  const probe: i32[] = [7];\n  const set: Set<i32> = new Set<i32>();\n  set.add(1);\n  set.forEach((key: i32): void => { print(`${probe[key + 1]}`); });\n}\n",
    ] {
        let files = [SourceFile::new("test.ts", src)];
        let mut outcomes = Vec::new();
        for (tier, result) in [
            ("dev-JIT", run_jit(&files)),
            ("ship-C-AOT", run_c_aot(&files)),
        ] {
            match result {
                Err(RunError::Trap(t)) => {
                    assert_eq!(t.rule, TrapKind::IndexOutOfBounds, "{tier}");
                    assert_eq!(t.pos.file, "test.ts", "{tier}");
                    assert_eq!(t.pos.line, 5, "{tier}");
                    outcomes.push(trap_outcome(t));
                }
                other => panic!("{tier}: expected an out-of-bounds trap, got {other:?}"),
            }
        }
        assert_trap_outcomes_identical("Map/Set forEach callback trap", &outcomes);
    }
}

#[test]
fn map_growth_during_for_each_preserves_the_fixed_entry_bound() {
    // The deleted first slot makes the ordered vector compactable. The
    // callback's insertion reaches the growth boundary while iteration
    // is positioned after that slot. The shared P22 traversal must keep
    // the live 2,3,4 suffix without extending the entry snapshot to 5.
    assert_tiers_print(
        "let seen: string = \"\";\n\
         export function main(): void {\n\
           const map: Map<i32, i32> = new Map<i32, i32>();\n\
           map.set(1, 10);\n\
           map.set(2, 20);\n\
           map.set(3, 30);\n\
           map.set(4, 40);\n\
           map.delete(1);\n\
           map.forEach((value: i32, key: i32): void => {\n\
             seen += `${key}:${value}|`;\n\
             if (key === 2) {\n\
               map.set(5, 50);\n\
             }\n\
           });\n\
           print(seen);\n\
         }\n",
        "2:20|3:30|4:40|\n",
    );
}

#[test]
fn map_mutation_during_for_each_keeps_the_p22_visit_rules() {
    assert_tiers_print(
        "let seen: string = \"\";\n\
         export function main(): void {\n\
           const inserted: Map<i32, i32> = new Map<i32, i32>();\n\
           inserted.set(1, 10);\n\
           inserted.set(2, 20);\n\
           inserted.forEach((value: i32, key: i32): void => {\n\
             seen += `${key}`;\n\
             if (key === 1) { inserted.set(3, 30); }\n\
           });\n\
           seen += \"|\";\n\
           const deleted: Map<i32, i32> = new Map<i32, i32>();\n\
           deleted.set(1, 10);\n\
           deleted.set(2, 20);\n\
           deleted.set(3, 30);\n\
           deleted.forEach((value: i32, key: i32): void => {\n\
             seen += `${key}`;\n\
             if (key === 1) { deleted.delete(2); }\n\
           });\n\
           seen += \"|\";\n\
           const cleared: Map<i32, i32> = new Map<i32, i32>();\n\
           cleared.set(1, 10);\n\
           cleared.set(2, 20);\n\
           cleared.forEach((value: i32, key: i32): void => {\n\
             seen += `${key}`;\n\
             cleared.clear();\n\
           });\n\
           seen += \"|\";\n\
           const removed: Map<i32, i32> = new Map<i32, i32>();\n\
           removed.set(1, 10);\n\
           removed.set(2, 20);\n\
           removed.forEach((value: i32, key: i32): void => {\n\
             seen += `${key}`;\n\
             Context.free(removed);\n\
           });\n\
           print(seen);\n\
         }\n",
        "12|13|1|1\n",
    );
}

#[test]
fn fill_reverse_and_sort_return_the_receiver_not_a_copy() {
    // stdlib.md §9: the in-place methods return the receiver. Mutating
    // through the returned handle must be visible through the original
    // one — a44/a45 cannot tell a fresh copy from the receiver, so the
    // expected bytes are asserted here rather than only cross-tier
    // agreement.
    assert_tiers_print(
        "export function main(): void {\n  const xs: i32[] = [];\n  xs.push(3);\n  xs.push(1);\n  xs.push(2);\n  const rev: i32[] = xs.reverse();\n  rev.push(9);\n  print(xs.join(\",\"));\n  const filled: i32[] = xs.fill(7, 0, 1);\n  filled.push(8);\n  print(xs.join(\",\"));\n  const sorted: i32[] = xs.sort((a: i32, b: i32): i32 => a - b);\n  sorted.push(0);\n  print(`${xs.join(\",\")} ${xs.length}`);\n}\n",
        "2,1,3,9\n7,1,3,9,8\n1,3,7,8,9,0 6\n",
    );
}

#[test]
fn join_prints_negative_zero_as_the_q14_rules_require() {
    // Q14 formatting, not the host's: `-0` keeps its sign in `join`
    // exactly as in interpolation. Node 24.18.0 prints `0.1,2.5,0` for
    // the same array (run 2026-07-25) — a recorded divergence, and one
    // no committed golden pins.
    assert_tiers_print(
        "export function main(): void {\n  const xs: f64[] = [0.1, 2.5, -0];\n  print(xs.join(\",\"));\n}\n",
        "0.1,2.5,-0\n",
    );
}

// ----- P11 phase-review regressions: evaluation order (CRITICAL 1) -----
//
// TS/JS evaluate a method call's receiver before its arguments. The dev
// JIT does so by construction (SSA order); the ship tier must bind the
// receiver to a temporary before it emits any argument statement, or C's
// statement order runs the argument first. Each program below logs the
// order it observed, so the two tiers disagree unless the property holds.

#[test]
fn array_needle_method_evaluates_the_receiver_before_the_argument() {
    assert_tiers_print(
        "let log: string = \"\";\nfunction mkArr(): i32[] {\n  log = log + \"R\";\n  const a: i32[] = [];\n  a.push(1);\n  a.push(2);\n  return a;\n}\nfunction mkNeedle(): i32 {\n  log = log + \"N\";\n  return 2;\n}\nexport function main(): void {\n  const r: i32 = mkArr().indexOf(mkNeedle());\n  print(`${log}:${r}`);\n}\n",
        "RN:1\n",
    );
}

#[test]
fn array_closure_method_evaluates_the_receiver_before_the_callback() {
    assert_tiers_print(
        "let log: string = \"\";\nfunction mkArr(): i32[] {\n  log = log + \"R\";\n  const a: i32[] = [];\n  a.push(1);\n  a.push(2);\n  return a;\n}\nfunction big(v: i32): boolean {\n  return v > 1;\n}\nfunction mkPred(): (v: i32) => boolean {\n  log = log + \"P\";\n  return big;\n}\nexport function main(): void {\n  const kept: i32[] = mkArr().filter(mkPred());\n  print(`${log}:${kept.length}`);\n}\n",
        "RP:1\n",
    );
}

#[test]
fn array_reduce_evaluates_receiver_then_callback_then_init() {
    assert_tiers_print(
        "let log: string = \"\";\nfunction mkArr(): i32[] {\n  log = log + \"R\";\n  const a: i32[] = [];\n  a.push(1);\n  a.push(2);\n  return a;\n}\nfunction add(acc: i32, v: i32): i32 {\n  return acc + v;\n}\nfunction mkStep(): (acc: i32, v: i32) => i32 {\n  log = log + \"F\";\n  return add;\n}\nfunction mkInit(): i32 {\n  log = log + \"I\";\n  return 10;\n}\nexport function main(): void {\n  const total: i32 = mkArr().reduce(mkStep(), mkInit());\n  print(`${log}:${total}`);\n}\n",
        "RFI:13\n",
    );
}

#[test]
fn array_push_evaluates_the_receiver_before_the_argument() {
    assert_tiers_print(
        "let log: string = \"\";\nfunction mkArr(): i32[] {\n  log = log + \"R\";\n  const a: i32[] = [];\n  a.push(1);\n  return a;\n}\nfunction mkVal(): i32 {\n  log = log + \"V\";\n  return 5;\n}\nexport function main(): void {\n  mkArr().push(mkVal());\n  print(log);\n}\n",
        "RV\n",
    );
}

#[test]
fn string_method_evaluates_the_receiver_before_the_argument() {
    // The argument emits statements of its own (`reverse` is an in-place
    // call statement), so an unbound receiver expression would land in
    // the call after them.
    assert_tiers_print(
        "let log: string = \"\";\nfunction mkStr(): string {\n  log = log + \"R\";\n  return \"2,1\";\n}\nfunction mkArr(): i32[] {\n  log = log + \"A\";\n  const a: i32[] = [];\n  a.push(1);\n  a.push(2);\n  return a;\n}\nexport function main(): void {\n  const hit: boolean = mkStr().includes(mkArr().reverse().join(\",\"));\n  print(`${log}:${hit}`);\n}\n",
        "RA:true\n",
    );
}

// ----- evaluation order: the remaining operand sites -----
//
// The same property as the array/string sites above, one test per site
// class: every sub-expression of one C expression is evaluated left to
// right, matching the dev tier (`lower/func.rs`), instead of resting on
// C's unspecified operand order.
//
// The right-hand operand is spelled `pick ? mkR() : 0` on purpose: a
// ternary lowers to `if`/`else` **statements**, so its side effect is
// hoisted above the enclosing C expression unless the operands to its
// left are bound first. That makes the order deterministic to observe —
// a plain call as the operand would only measure whichever order the C
// compiler happens to pick today.

/// Side-effecting helpers shared by the order tests: each appends its
/// tag to `log`, so the printed log is the observed evaluation order.
const ORDER_PRELUDE: &str = "let log: string = \"\";\nlet pick: boolean = true;\nfunction note(tag: string): void {\n  log = log + tag;\n}\nfunction mkL(): i32 {\n  note(\"L\");\n  return 1;\n}\nfunction mkR(): i32 {\n  note(\"R\");\n  return 2;\n}\nfunction take(a: i32, b: i32): i32 {\n  return a * 10 + b;\n}\n";

/// A program built on [`ORDER_PRELUDE`], asserted on both tiers.
fn assert_order(body: &str, expected: &str) {
    assert_tiers_print(&format!("{ORDER_PRELUDE}{body}"), expected);
}

#[test]
fn user_function_arguments_run_left_to_right() {
    assert_order(
        "export function main(): void {\n  const r: i32 = take(mkL(), pick ? mkR() : 0);\n  print(`${log}:${r}`);\n}\n",
        "LR:12\n",
    );
}

#[test]
fn indirect_call_arguments_run_left_to_right() {
    assert_order(
        "export function main(): void {\n  const f: (a: i32, b: i32) => i32 = take;\n  const r: i32 = f(mkL(), pick ? mkR() : 0);\n  print(`${log}:${r}`);\n}\n",
        "LR:12\n",
    );
}

#[test]
fn reference_class_method_receiver_runs_before_its_argument() {
    assert_order(
        "class Box {\n  n: i32;\n  constructor(n: i32) {\n    this.n = n;\n  }\n  add(v: i32): i32 {\n    return this.n + v;\n  }\n}\nfunction mkBox(): Box {\n  note(\"B\");\n  return new Box(10);\n}\nexport function main(): void {\n  const r: i32 = mkBox().add(pick ? mkR() : 0);\n  print(`${log}:${r}`);\n}\n",
        "BR:12\n",
    );
}

#[test]
fn value_class_method_receiver_runs_before_its_argument() {
    assert_order(
        "@CStruct\nclass P {\n  n: i32;\n  constructor(n: i32) {\n    this.n = n;\n  }\n  add(v: i32): i32 {\n    return this.n + v;\n  }\n}\nfunction mkP(): P {\n  note(\"P\");\n  return new P(10);\n}\nexport function main(): void {\n  const r: i32 = mkP().add(pick ? mkR() : 0);\n  print(`${log}:${r}`);\n}\n",
        "PR:12\n",
    );
}

#[test]
fn constructor_arguments_run_left_to_right() {
    assert_order(
        "class Pair {\n  a: i32;\n  b: i32;\n  constructor(a: i32, b: i32) {\n    this.a = a;\n    this.b = b;\n  }\n}\nexport function main(): void {\n  const p: Pair = new Pair(mkL(), pick ? mkR() : 0);\n  print(`${log}:${p.a}${p.b}`);\n}\n",
        "LR:12\n",
    );
}

#[test]
fn math_arguments_run_left_to_right() {
    assert_order(
        "export function main(): void {\n  const r: f64 = Math.max(mkL() as f64, (pick ? mkR() : 0) as f64);\n  print(`${log}:${r}`);\n}\n",
        "LR:2\n",
    );
}

#[test]
fn date_utc_arguments_run_left_to_right() {
    assert_order(
        "export function main(): void {\n  const ms: i64 = Date.UTC(2000, mkL(), pick ? mkR() : 0);\n  print(`${log}:${new Date(ms).toISOString()}`);\n}\n",
        "LR:2000-02-02T00:00:00.000Z\n",
    );
}

#[test]
fn binary_operands_run_left_to_right() {
    assert_order(
        "export function main(): void {\n  const r: i32 = mkL() + (pick ? mkR() : 0);\n  print(`${log}:${r}`);\n}\n",
        "LR:3\n",
    );
}

#[test]
fn index_operands_run_left_to_right() {
    assert_order(
        "function mkArr(): i32[] {\n  note(\"A\");\n  const a: i32[] = [];\n  a.push(7);\n  a.push(8);\n  return a;\n}\nexport function main(): void {\n  const r: i32 = mkArr()[(pick ? mkR() : 0) - 1];\n  print(`${log}:${r}`);\n}\n",
        "AR:8\n",
    );
}

#[test]
fn array_element_store_evaluates_the_target_before_the_value() {
    assert_order(
        "function mkArr(): i32[] {\n  note(\"A\");\n  const a: i32[] = [];\n  a.push(7);\n  a.push(8);\n  return a;\n}\nexport function main(): void {\n  mkArr()[0] = pick ? mkR() : 0;\n  print(log);\n}\n",
        "AR\n",
    );
}

#[test]
fn field_store_evaluates_the_target_base_before_the_value() {
    assert_order(
        "class Box {\n  n: i32;\n  constructor(n: i32) {\n    this.n = n;\n  }\n}\nfunction mkBox(): Box {\n  note(\"B\");\n  return new Box(0);\n}\nexport function main(): void {\n  mkBox().n = pick ? mkR() : 0;\n  print(log);\n}\n",
        "BR\n",
    );
}

#[test]
fn compound_assignment_evaluates_the_target_base_once() {
    // `mkBox().n += …` calls `mkBox` exactly once, as the dev tier does;
    // an unpinned place is spelled twice in the emitted C.
    assert_order(
        "class Box {\n  n: i32;\n  constructor(n: i32) {\n    this.n = n;\n  }\n}\nfunction mkBox(): Box {\n  note(\"B\");\n  return new Box(5);\n}\nexport function main(): void {\n  mkBox().n += mkL();\n  print(log);\n}\n",
        "BL\n",
    );
}

#[test]
fn fixed_array_literal_elements_run_left_to_right() {
    assert_order(
        "export function main(): void {\n  const fa: FixedArray<i32, 2> = [mkL(), pick ? mkR() : 0];\n  print(`${log}:${fa[0]}${fa[1]}`);\n}\n",
        "LR:12\n",
    );
}

#[test]
fn short_circuit_operands_do_not_run_the_skipped_side() {
    // `&&`/`||` skip the right operand entirely (the dev tier branches).
    // The right operand here lowers to statements, which must not be
    // hoisted out of the branch in the ship tier.
    assert_order(
        "export function main(): void {\n  const off: boolean = mkL() > 100;\n  const both: boolean = off && (pick ? mkR() : 0) > 0;\n  const either: boolean = !off || (pick ? mkR() : 0) > 0;\n  print(`${log}:${both}${either}`);\n}\n",
        "L:falsetrue\n",
    );
}

#[test]
fn short_circuit_in_a_loop_condition_re_runs_per_iteration() {
    // The branch lowering above sits inside the loop, so the condition
    // is re-evaluated each iteration and still guards its right operand:
    // the last test would index out of bounds if `&&` did not stop.
    assert_tiers_print(
        "export function main(): void {\n  const xs: i32[] = [];\n  xs.push(1);\n  xs.push(2);\n  xs.push(3);\n  let i: i32 = 0;\n  let sum: i32 = 0;\n  while (i < xs.length && xs[i] < 3) {\n    sum = sum + xs[i];\n    i = i + 1;\n  }\n  let j: i32 = 0;\n  let seen: i32 = 0;\n  while (j >= xs.length || xs[j] > 0) {\n    seen = seen + 1;\n    j = j + 1;\n    if (j > 2) {\n      break;\n    }\n  }\n  print(`${i} ${sum} ${j} ${seen}`);\n}\n",
        "2 3 3 3\n",
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
fn allocation_metadata_regenerates_byte_identically() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = include_str!("../../corpus/accept/a15-manual-lifetime.ts");
    let hir = check_program(&[SourceFile::new("a15-manual-lifetime.ts", source)])
        .expect("metadata fixture checks");
    let program = emit_c(&hir).expect("metadata fixture emits");
    assert_eq!(
        program.allocation_metadata_header.as_bytes(),
        include_bytes!("fixtures/p21-allocation-metadata.h"),
        "generated allocation metadata header drifted"
    );
    assert_eq!(
        program.allocation_metadata_source.as_bytes(),
        include_bytes!("fixtures/p21-allocation-metadata.inc"),
        "generated allocation class/position tables drifted"
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
    // `subscript_rt_ctx_set_now(ctx, PINNED_MS)` before any program code
    // runs. The harness's own entry is untouched.
    use std::path::PathBuf;
    use std::process::Command;
    use subscript_codegen::{emit_c, runtime_staticlib_path, tool_output_report, AOT_ENTRY_C};
    use subscript_compiler::check_program;

    const PINNED_MS: i64 = 1_592_224_496_789;
    const PROGRAM: &str = "export function main(): void {\n  const t: i64 = Date.now();\n  print(`${t}`);\n  print(new Date(Date.now()).toISOString());\n}\n";
    const EXPECTED: &[u8] = b"1592224496789\n2020-06-15T12:34:56.789Z\n";

    let hir = check_program(&[SourceFile::new("test.ts", PROGRAM)]).expect("checks clean");
    let program = emit_c(&hir).expect("ship C emission");
    let staticlib = runtime_staticlib_path().expect("runtime staticlib");

    let call_anchor = "    call_script_entry(ctx, subscript_init);";
    assert!(
        AOT_ENTRY_C.contains(call_anchor),
        "AOT_ENTRY_C anchors moved; update this test's entry derivation"
    );
    let entry = AOT_ENTRY_C.replace(
        call_anchor,
        &format!(
            "    subscript_rt_ctx_set_now(ctx, {PINNED_MS});\n    call_script_entry(ctx, subscript_init);"
        ),
    );

    // Temp dir removed on every exit path, including assertion panics.
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir =
        std::env::temp_dir().join(format!("subscript-cemit-pinned-now-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let _cleanup = Cleanup(dir.clone());

    let src_path = dir.join("program.c");
    let entry_path = dir.join("entry.c");
    let exe_path = dir.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&src_path, program.source.as_bytes()).expect("write program.c");
    std::fs::write(&entry_path, entry.as_bytes()).expect("write entry.c");

    // Same compile line as `run_c_aot` (§11/§11c): the platform C
    // compiler at C11 -O2, and the runtime staticlib. This program has
    // no foreign calls, so it supplies no native library. On windows-msvc
    // the compiler is MSVC `cl`, resolved with its toolchain environment
    // from the registry (§11c); on every other host it is clang.
    #[cfg(all(windows, target_env = "msvc"))]
    let compile = {
        use std::ffi::OsString;
        let system_libs: &[&str] = &[
            "kernel32.lib",
            "ntdll.lib",
            "userenv.lib",
            "ws2_32.lib",
            "dbghelp.lib",
        ];
        let mut command = if let Some(cc) = std::env::var_os("CC") {
            Command::new(cc)
        } else {
            let target = target_lexicon::HOST.to_string();
            let tool = cc::windows_registry::find_tool(&target, "cl.exe").expect(
                "MSVC cl.exe (install the Visual C++ build tools or set $CC; compiler.md §11c)",
            );
            let mut command = Command::new(tool.path());
            command.envs(tool.env().iter().cloned());
            command
        };
        let mut object_dir_arg = OsString::from("/Fo:");
        object_dir_arg.push(dir.as_os_str());
        object_dir_arg.push(std::path::MAIN_SEPARATOR.to_string());
        let mut exe_arg = OsString::from("/Fe:");
        exe_arg.push(exe_path.as_os_str());
        command
            .args(["/nologo", "/std:c11", "/O2", "/utf-8", "/fp:strict"])
            .arg(object_dir_arg)
            .arg(&src_path)
            .arg(&entry_path)
            .arg(&staticlib)
            .args(system_libs)
            .arg(exe_arg)
            .arg("-link")
            .output()
            .expect("run the C compiler (cl; set $CC)")
    };
    #[cfg(not(all(windows, target_env = "msvc")))]
    let compile = {
        let compiler = host_c_compiler().expect("resolve the host C compiler");
        compiler
            .command()
            .arg("-std=c11")
            .arg("-O2")
            .arg("-fwrapv")
            .arg("-ffp-contract=off")
            .arg(&src_path)
            .arg(&entry_path)
            .arg(&staticlib)
            .args(runtime_system_libraries(compiler.style()))
            .arg("-o")
            .arg(&exe_path)
            .output()
            .expect("run the C compiler (clang; set $CC)")
    };
    assert!(
        compile.status.success(),
        "compiling/linking the emitted C failed:\n{}",
        tool_output_report(&compile)
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("run linked program");
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
    let files = [SourceFile::new(
        "test.ts",
        "function at(xs: FixedArray<i32, 3>, i: i32): i32 {\n  return xs[i];\n}\nexport function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${at(xs, 5)}`);\n}\n",
    )];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::IndexOutOfBounds, "{tier}");
                assert_eq!(t.pos.file, "test.ts", "{tier}");
                assert_eq!(t.pos.line, 2, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected an out-of-bounds trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("FixedArray index trap", &outcomes);
}

#[test]
fn ship_c_aot_reports_a_division_by_zero_trap() {
    let files = [SourceFile::new(
        "test.ts",
        "function f(d: i32): i32 {\n  return 10 / d;\n}\nexport function main(): void {\n  print(`${f(0)}`);\n}\n",
    )];
    let mut outcomes = Vec::new();
    for (tier, result) in [
        ("dev-JIT", run_jit(&files)),
        ("ship-C-AOT", run_c_aot(&files)),
    ] {
        match result {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::DivisionByZero, "{tier}");
                assert_eq!(t.pos.line, 2, "{tier}");
                outcomes.push(trap_outcome(t));
            }
            other => panic!("{tier}: expected a division-by-zero trap, got {other:?}"),
        }
    }
    assert_trap_outcomes_identical("division-by-zero trap", &outcomes);
}
