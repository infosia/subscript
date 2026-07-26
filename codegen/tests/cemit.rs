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

#[path = "support/trap_corpus.rs"]
mod trap_corpus;

use subscript_codegen::{run_c_aot, run_jit, RunError, TrapReport};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

type TrapOutcome = ((TrapKind, String, subscript_compiler::Pos), Vec<u8>);

fn trap_outcome(report: TrapReport) -> TrapOutcome {
    (
        (report.rule, report.message, report.pos),
        report.stdout,
    )
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
    let mut outcomes = Vec::new();
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    assert_eq!(
        ids.len(),
        6,
        "expected exactly the six committed trap entries (t01 P13 JsonResult.value + five P19 \
         unwind probes), found {}",
        ids.len()
    );

    let mut divergences = Vec::new();
    for id in ids {
        let files = trap_corpus::trap_sources(&trap, &id);
        let expected = trap_corpus::trap_expected(&trap, &id);
        let mut outcomes = Vec::new();
        for (tier, result) in [
            ("dev-JIT", run_jit(&files)),
            ("ship-C-AOT", run_c_aot(&files)),
        ] {
            match result {
                Err(RunError::Trap(report)) => {
                    assert_eq!(report.pos.file, format!("{id}.ts"), "{tier}: {id}");
                    if id == "t01-json-result-value" {
                        assert_eq!(report.rule, TrapKind::JsonResultValue, "{tier}: {id}");
                        assert_eq!(
                            report.message,
                            "`JsonResult.value` read when `ok` is false",
                            "{tier}: {id}"
                        );
                        assert_eq!(report.pos.line, 9, "{tier}: {id}");
                    } else {
                        assert_eq!(report.rule, TrapKind::IndexOutOfBounds, "{tier}: {id}");
                    }
                    outcomes.push(trap_outcome(report));
                }
                other => {
                    panic!("{tier}: {id}: expected a runtime trap, got {other:?}")
                }
            }
        }
        assert_eq!(
            outcomes[0].0, outcomes[1].0,
            "{id}: tiers disagree on the trap tuple"
        );
        assert_eq!(
            outcomes[0].1,
            expected,
            "{id}: dev-JIT stdout differs from its dev-generated .expected\n  dev-JIT stdout = \
             {:?}\n  expected stdout = {:?}",
            String::from_utf8_lossy(&outcomes[0].1),
            String::from_utf8_lossy(&expected)
        );
        if outcomes[1].1 != expected {
            divergences.push(format!(
                "{id}: dev-JIT/.expected = {:?}, ship-C-AOT = {:?}",
                String::from_utf8_lossy(&expected),
                String::from_utf8_lossy(&outcomes[1].1)
            ));
        }
    }
    assert!(
        divergences.is_empty(),
        "trap corpus stdout divergences:\n{}",
        divergences.join("\n")
    );
}

#[test]
fn out_of_range_320_byte_cstruct_store_stops_before_the_store() {
    // P19: the old ship-tier path let ss_arr_at return its 256-byte
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
fn acyclic_json_serializer_emits_no_tracking_operations() {
    use subscript_codegen::emit_c;
    use subscript_compiler::check_program;

    let source = "class Box {\n  value: i32;\n  constructor(value: i32) { this.value = value; }\n}\nexport function main(): void {\n  print(JSON.stringify(new Box(7)));\n}\n";
    let hir = check_program(&[SourceFile::new("test.ts", source)]).expect("checks clean");
    let c = emit_c(&hir).expect("emit C").source;
    assert!(c.contains("sub_rt_json_begin(ctx,"), "{c}");
    assert!(!c.contains("sub_rt_json_begin_tracked(ctx,"), "{c}");
    assert!(!c.contains("sub_rt_json_visit(ctx,"), "{c}");
    assert!(!c.contains("sub_rt_json_leave(ctx,"), "{c}");
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
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    const PROLOGUE: &str = "let sink: i32 = 0;\nexport function main(): void {\n  const xs: i32[] = [1, 2, 3];\n";
    for call in [
        "xs.forEach((v: i32): void => { sink = sink + xs[v + 1]; });",
        "sink = xs.filter((v: i32): boolean => xs[v + 1] > 0).length;",
        "sink = xs.reduce((acc: i32, v: i32): i32 => acc + xs[v + 1], 0);",
        "sink = xs.some((v: i32): boolean => xs[v + 1] > 5) ? 1 : 0;",
        "sink = xs.every((v: i32): boolean => xs[v + 1] > 0) ? 1 : 0;",
        "sink = xs.findIndex((v: i32): boolean => xs[v + 1] > 5);",
        "sink = xs.sort((a: i32, b: i32): i32 => xs[a + b] - 1).length;",
    ] {
        assert_callback_trap_identical(&format!("{PROLOGUE}  {call}\n  print(`${{sink}}`);\n}}\n"), 4);
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
    let accept =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
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
fn map_growth_during_for_each_does_not_compact_under_the_cursor() {
    // The deleted first slot makes the ordered vector compactable. The
    // callback's insertion reaches the growth boundary while iteration
    // is positioned after that slot; moving entries here would skip key
    // 3. Both tiers must preserve JS/Node's 2,3,4,5 visit sequence.
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
        "2:20|3:30|4:40|5:50|\n",
    );
}

#[test]
fn map_mutation_during_for_each_keeps_the_pinned_visit_rules() {
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
             unsafeDelete(removed);\n\
           });\n\
           print(seen);\n\
         }\n",
        "123|13|1|1\n",
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

    let call_anchor = "    call_script_entry(ctx, ss_init);";
    assert!(
        AOT_ENTRY_C.contains(call_anchor),
        "AOT_ENTRY_C anchors moved; update this test's entry derivation"
    );
    let entry = AOT_ENTRY_C.replace(
        call_anchor,
        &format!(
            "    sub_rt_ctx_set_now(ctx, {PINNED_MS});\n    call_script_entry(ctx, ss_init);"
        ),
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
    let files = [SourceFile::new(
        "test.ts",
        "function at(xs: FixedArray<i32, 3>, i: i32): i32 {\n  return xs[i];\n}\nexport function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${at(xs, 5)}`);\n}\n",
    )];
    let mut outcomes = Vec::new();
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
    for (tier, result) in [("dev-JIT", run_jit(&files)), ("ship-C-AOT", run_c_aot(&files))] {
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
