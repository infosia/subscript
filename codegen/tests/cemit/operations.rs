use super::*;

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
fn map_growth_during_for_each_visits_the_appended_entry() {
    // The deleted first slot makes the ordered vector compactable. The
    // callback's insertion reaches the growth boundary while iteration
    // is positioned after that slot. The live ECMA traversal must keep
    // the 2,3,4 suffix and extend it to the appended entry 5.
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
    // Section 95 uses the same zero spelling in join and interpolation.
    assert_tiers_print(
        "export function main(): void {\n  const xs: f64[] = [0.1, 2.5, -0];\n  print(xs.join(\",\"));\n}\n",
        "0.1,2.5,0\n",
    );
}

// ----- evaluation order: the receiver before the arguments -----
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
        "@ValueType\nclass P {\n  n: i32;\n  constructor(n: i32) {\n    this.n = n;\n  }\n  add(v: i32): i32 {\n    return this.n + v;\n  }\n}\nfunction mkP(): P {\n  note(\"P\");\n  return new P(10);\n}\nexport function main(): void {\n  const r: i32 = mkP().add(pick ? mkR() : 0);\n  print(`${log}:${r}`);\n}\n",
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
const A22_SOURCE: &str = include_str!("../../../corpus/accept/a22-matrix-propagation.ts");
const A22_GOLDEN: &[u8] = include_bytes!("../../../corpus/accept/a22-matrix-propagation.expected");

fn emitted_function_body(c: &str, function: subscript_compiler::lir::FunctionId) -> &str {
    let name = format!("sub_f{}(", function.0);
    let signature = c
        .lines()
        .find(|line| line.starts_with("static ") && line.contains(&name) && line.ends_with(" {"))
        .unwrap_or_else(|| panic!("emitted C has no body for {name}:\n{c}"));
    let start = c
        .find(signature)
        .unwrap_or_else(|| panic!("emitted C lost the located signature `{signature}`"));
    let rest = &c[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("emitted C has no end for {name}:\n{rest}"));
    &rest[..end + 3]
}

#[test]
fn dynamic_array_length_and_index_use_inline_header_fields() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    const SOURCE: &str = "function probe(xs: i32[], i: i32): i32 {\n  if (i < xs.length) {\n    return xs[i];\n  }\n  return -1;\n}\nexport function main(): void {\n  const xs: i32[] = [7];\n  print(`${probe(xs, 0)},${probe(xs, 1)}`);\n}\n";
    let files = [SourceFile::new("inline-array-header.ts", SOURCE)];
    let hir = check_program(&files).expect("inline array-header probe checks cleanly");
    let lir = lower_module(&hir).expect("inline array-header probe lowers to LIR");
    let probe = lir
        .functions
        .iter()
        .find(|function| function.source_name == "probe")
        .expect("program has probe");
    let c = emit_c(&hir)
        .expect("inline array-header probe emits C")
        .source;
    let body = emitted_function_body(&c, probe.id);

    for opaque in ["subscript_rt_array_len", "subscript_rt_array_data"] {
        assert!(
            !body.contains(opaque),
            "dynamic array access retained opaque helper `{opaque}`:\n{body}"
        );
    }
    assert!(
        body.contains("(int32_t)(((SsArrayHeader*)") && body.contains("->len)"),
        "dynamic Length is not an inline header read:\n{body}"
    );
    let captured_length = body
        .lines()
        .find(|line| line.contains("uint64_t t") && line.contains("->len;"))
        .expect("checked index does not capture its length once");
    let length_name = captured_length
        .split_whitespace()
        .nth(1)
        .expect("captured length has no C identifier");
    let trap_branch = body
        .lines()
        .find(|line| line.contains("subscript_rt_trap_index_out_of_bounds"))
        .expect("checked index has no trap branch");
    assert!(
        trap_branch.contains(&format!("(uint64_t)({length_name})"))
            && trap_branch.contains(&format!("(uint32_t)({length_name})"))
            && !trap_branch.contains("->len"),
        "bounds test and diagnostic do not share the captured length:\n{trap_branch}"
    );
    assert!(
        body.lines()
            .any(|line| line.contains("->data +") && line.contains("->elem_size")),
        "dynamic AddressOfIndex is not inline header arithmetic:\n{body}"
    );
}

#[test]
fn static_array_callback_iterator_uses_inline_header_fields() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    const SOURCE: &str = "function twice(value: i32): i32 {\n  return value * 2;\n}\nexport function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  print(`${xs.map(twice)[0]}`);\n}\n";
    let files = [SourceFile::new("inline-array-iterator.ts", SOURCE)];
    let hir = check_program(&files).expect("static callback iterator probe checks cleanly");
    let lir = lower_module(&hir).expect("static callback iterator probe lowers to LIR");
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("program has main");
    let callback = lir
        .functions
        .iter()
        .find(|function| function.source_name == "twice")
        .expect("program has twice");
    let c = emit_c(&hir)
        .expect("static callback iterator probe emits C")
        .source;
    let body = emitted_function_body(&c, main.id);

    for opaque in ["subscript_rt_array_len", "subscript_rt_array_data"] {
        assert!(
            !body.contains(opaque),
            "static callback iterator retained opaque helper `{opaque}`:\n{body}"
        );
    }
    assert!(
        body.contains("((const SsArrayHeader*)")
            && body.contains("->len")
            && body.contains("->data"),
        "static callback iterator does not read the array header inline:\n{body}"
    );
    assert!(
        body.contains("subscript_rt_array_with_capacity")
            && body.contains(&format!("sub_f{}(ctx", callback.id.0)),
        "static callback map lost its pre-sized output or direct call:\n{body}"
    );
    let lines = body.lines().collect::<Vec<_>>();
    let call = lines
        .iter()
        .position(|line| line.contains(&format!("sub_f{}(ctx", callback.id.0)))
        .expect("static callback map has a direct callback call");
    assert!(
        !lines[call + 1].contains("*(const uint32_t*)ctx"),
        "non-trapping direct callback retained a pending-trap branch:\n{body}"
    );
    assert!(
        !body
            .lines()
            .any(|line| line.contains(".position = (") && line.contains(".position + 1ull")),
        "unused forward IteratorAdvance retained its aggregate update:\n{body}"
    );
    let push_header = lines[call + 1..]
        .iter()
        .position(|line| line.contains("SsArrayHeader*"))
        .map(|offset| call + 1 + offset)
        .expect("static callback map has an inline push header");
    assert!(
        !lines[push_header + 1].contains("->len <"),
        "preallocated static-map push retained its growth branch:\n{body}"
    );
}

#[test]
fn generator_creation_call_keeps_its_implicit_allocation_check() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    const SOURCE: &str = "function* values(): Generator<i32> {\n  yield 1;\n}\nexport function main(): void {\n  const generator: Generator<i32> = values();\n  print(`${generator.next().value}`);\n}\n";
    let files = [SourceFile::new("generator-allocation.ts", SOURCE)];
    let hir = check_program(&files).expect("generator allocation probe checks cleanly");
    let lir = lower_module(&hir).expect("generator allocation probe lowers to LIR");
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("program has main");
    let generator = lir
        .functions
        .iter()
        .find(|function| function.source_name == "values")
        .expect("program has values");
    let c = emit_c(&hir)
        .expect("generator allocation probe emits C")
        .source;
    let body = emitted_function_body(&c, main.id);
    let lines = body.lines().collect::<Vec<_>>();
    let call = lines
        .iter()
        .position(|line| line.contains(&format!("sub_f{}(ctx", generator.id.0)))
        .expect("main calls the generator creator");
    assert!(
        lines[call + 1].contains("*(const uint32_t*)ctx"),
        "generator creation lost its implicit allocation check:\n{body}"
    );
}

#[test]
fn local_load_store_address_chains_emit_as_member_expressions() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    let files = [SourceFile::new("a22-matrix-propagation.ts", A22_SOURCE)];
    let hir = check_program(&files).expect("a22 checks cleanly");
    let lir = lower_module(&hir).expect("a22 lowers to LIR");
    let multiply = lir
        .functions
        .iter()
        .find(|function| function.source_name == "multiply")
        .expect("a22 has multiply");
    let c = emit_c(&hir).expect("a22 emits C").source;
    let body = emitted_function_body(&c, multiply.id);

    assert!(
        body.contains("((v0).d3).a["),
        "left matrix did not fold into its parameter value:\n{body}"
    );
    assert!(
        body.contains("((v1).d3).a["),
        "right matrix did not fold into its parameter value:\n{body}"
    );
    for redundant in ["SubC1 l0", "SubC1 l1", "SubFA_f32_16 l2", " = &l"] {
        assert!(
            !body.contains(redundant),
            "folded address retained local storage `{redundant}`:\n{body}"
        );
    }
    for initialized in [
        "SubC1 v0 = a0;",
        "SubC1 v1 = a1;",
        "SubFA_f32_16 v49 = v2;",
        "SubC1 v50 = *(v48);",
    ] {
        assert!(
            body.contains(initialized),
            "whole first write did not initialize `{initialized}`:\n{body}"
        );
    }
    for redundant_zero in ["SubC1 v0 = (SubC1){0};", "SubC1 v1 = (SubC1){0};"] {
        assert!(
            !body.contains(redundant_zero),
            "parameter retained redundant zero `{redundant_zero}`:\n{body}"
        );
    }
    assert!(
        body.contains("SubFA_f32_16 v2 = (SubFA_f32_16){ .a = {"),
        "fixed-array literal was not emitted as one initializer:\n{body}"
    );
    assert!(
        !body.contains("v2.a["),
        "fixed-array literal retained per-element stores:\n{body}"
    );
}

#[test]
fn multiply_constant_trip_loop_emits_four_straight_iterations() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    let files = [SourceFile::new("a22-matrix-propagation.ts", A22_SOURCE)];
    let hir = check_program(&files).expect("a22 checks cleanly");
    let lir = lower_module(&hir).expect("a22 lowers to LIR");
    let multiply = lir
        .functions
        .iter()
        .find(|function| function.source_name == "multiply")
        .expect("a22 has multiply");
    let c = emit_c(&hir).expect("a22 emits C").source;
    let body = emitted_function_body(&c, multiply.id);
    let iterations = multiply
        .blocks
        .iter()
        .filter(|block| {
            block
                .source_name
                .as_deref()
                .is_some_and(|name| name.starts_with("for.unrolled."))
        })
        .collect::<Vec<_>>();
    assert_eq!(iterations.len(), 4, "a22 inner loop was not unrolled");
    assert!(iterations.iter().all(|block| {
        matches!(
            block.terminator,
            subscript_compiler::lir::Terminator::Branch(_)
        )
    }));
    assert_eq!(
        body.matches("((v0).d3).a[").count(),
        4,
        "left matrix does not have four straight accesses:\n{body}"
    );
    assert_eq!(
        body.matches("((v1).d3).a[").count(),
        4,
        "right matrix does not have four straight accesses:\n{body}"
    );
}

#[test]
fn parallel_copy_cycle_uses_one_temporary() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    const SOURCE: &str = "export function main(): void {\n  let left: i32 = 1;\n  let right: i32 = 2;\n  let count: i32 = 0;\n  while (count < 1) {\n    const oldLeft: i32 = left;\n    left = right;\n    right = oldLeft;\n    count = count + 1;\n  }\n  print(`${left},${right}`);\n}\n";
    assert_tiers_print(SOURCE, "2,1\n");

    let files = [SourceFile::new("parallel-copy-cycle.ts", SOURCE)];
    let hir = check_program(&files).expect("parallel-copy cycle checks cleanly");
    let lir = lower_module(&hir).expect("parallel-copy cycle lowers to LIR");
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("program has main");
    let c = emit_c(&hir).expect("parallel-copy cycle emits C").source;
    let body = emitted_function_body(&c, main.id);
    let cycle_temporaries = body
        .lines()
        .filter(|line| line.trim_start().starts_with("int32_t t") && line.contains(" = v"))
        .count();
    assert_eq!(
        cycle_temporaries, 1,
        "the swap cycle must use one temporary:\n{body}"
    );
}

#[test]
fn address_passed_to_a_call_stays_materialized() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    let source = include_str!("../../../corpus/accept/a21-methods.ts");
    let files = [SourceFile::new("a21-methods.ts", source)];
    let hir = check_program(&files).expect("a21 checks cleanly");
    let lir = lower_module(&hir).expect("a21 lowers to LIR");
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("a21 has main");
    let point = main
        .locals
        .iter()
        .find(|local| local.source_name == "point")
        .expect("a21 main stores point locally");
    let c = emit_c(&hir).expect("a21 emits C").source;
    let body = emitted_function_body(&c, main.id);

    assert!(
        body.contains(&format!(" = &l{};", point.id.0)),
        "the method receiver address must stay materialized:\n{body}"
    );
}

#[test]
fn parameter_storage_is_initialized_once_when_its_address_escapes() {
    use subscript_codegen::emit_c;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::check_program;

    const SOURCE: &str = "@ValueType\nclass Point {\n  x: f32;\n  constructor(x: f32) { this.x = x; }\n  value(): f32 { return this.x; }\n}\nfunction pointValue(point: Point): f32 { return point.value(); }\nexport function main(): void { print(`${pointValue(new Point(3.0))}`); }\n";
    let files = [SourceFile::new("parameter-storage.ts", SOURCE)];
    let hir = check_program(&files).expect("parameter-storage probe checks cleanly");
    let lir = lower_module(&hir).expect("parameter-storage probe lowers to LIR");
    let function = lir
        .functions
        .iter()
        .find(|function| function.source_name == "pointValue")
        .expect("probe has pointValue");
    let c = emit_c(&hir)
        .expect("parameter-storage probe emits C")
        .source;
    let body = emitted_function_body(&c, function.id);

    assert!(
        body.contains("SubC1 l0 = (SubC1){0};"),
        "escaping parameter lost addressable storage:\n{body}"
    );
    assert_eq!(
        body.matches("l0 = v0;").count(),
        1,
        "parameter-to-local copy must be emitted exactly once:\n{body}"
    );
}

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
    std::fs::write(src_path.with_file_name("program.h"), &program.host_header).unwrap();
    std::fs::write(&entry_path, entry.as_bytes()).expect("write entry.c");

    // Same compile line as `run_c_aot` (§11/§11c): the platform C
    // compiler at C11 -O2, and the runtime staticlib. This program has
    // no foreign calls, so it supplies no native library. On windows-msvc
    // the compiler is MSVC `cl`, resolved with its toolchain environment
    // from the registry (§11c); on every other host it is clang.
    #[cfg(all(windows, target_env = "msvc"))]
    let compile = {
        use std::ffi::OsString;
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
            .args(runtime_system_libraries(CCompilerStyle::Msvc))
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

#[test]
fn held_async_copy_pass_and_second_await_match_both_tiers() {
    let files = [SourceFile::new(
        "held-copy.ts",
        "async function work(): Promise<i32> {\n  await Context.suspend();\n  return 41;\n}\nasync function consume(handle: Promise<i32>): Promise<i32> {\n  return await handle;\n}\nexport async function main(): Promise<void> {\n  const first: Promise<i32> = work();\n  {\n    const second: Promise<i32> = first;\n    print(`copy=${await second}`);\n  }\n  print(`again=${await first}`);\n  const passed: Promise<i32> = work();\n  print(`pass=${await consume(passed)}`);\n}\n",
    )];
    let expected = b"copy=41\nagain=41\npass=41\n";
    let jit = run_jit(&files).expect("held-copy dev JIT");
    let ship = run_c_aot(&files).expect("held-copy ship C AOT");
    assert_eq!(jit, expected);
    assert_eq!(ship, expected);
}

#[test]
fn held_async_cached_reference_survives_collect_on_both_tiers() {
    let files = [SourceFile::new(
        "held-reference.ts",
        "async function work(): Promise<string> {\n  await Context.suspend();\n  return `value=${41}`;\n}\nexport async function main(): Promise<void> {\n  const first: Promise<string> = work();\n  {\n    const second: Promise<string> = first;\n    print(await second);\n  }\n  Context.collect();\n  print(await first);\n}\n",
    )];
    let expected = b"value=41\nvalue=41\n";
    let jit = run_jit(&files).expect("held-reference dev JIT");
    let ship = run_c_aot(&files).expect("held-reference ship C AOT");
    assert_eq!(jit, expected);
    assert_eq!(ship, expected);
}

#[test]
fn many_completed_async_calls_leave_no_frames_without_collect() {
    let files = [SourceFile::new(
        "many-awaits.ts",
        "async function work(value: i32): Promise<i32> { return value; }\nexport async function main(): Promise<void> {\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < 10000; i += 1) total += await work(i);\n}\n",
    )];
    let (output, accounting) =
        run_jit_with_memory_accounting(&files, false).expect("many-awaits dev JIT");
    assert!(output.is_empty());
    assert_eq!(
        accounting.live_bytes, 0,
        "each completed child and the async root must be released without Context.collect()"
    );
}

#[test]
fn a183_long_string_emits_five_adjacent_c_literals() {
    let source = include_str!("../../../corpus/accept/a183-long-string-literal.ts");
    let hir = check_program(&[SourceFile::new("a183-long-string-literal.ts", source)])
        .expect("a183 checks cleanly");
    let c = subscript_codegen::emit_c(&hir)
        .expect("a183 emits C")
        .source;
    let longest_line = c.lines().map(|line| line.chars().count()).max().unwrap();
    assert!(longest_line <= 16380, "longest C line: {longest_line}");

    let start = c.find("\"abababab").expect("long string literal");
    let end = start + c[start..].find(", 20000ull").expect("literal byte length");
    let pieces: Vec<_> = c[start..end].split('\n').collect();
    assert_eq!(pieces.len(), 5);
    for piece in &pieces {
        assert_eq!(piece.len(), 4002);
        assert!(piece.starts_with('"') && piece.ends_with('"'));
        assert_eq!(piece.matches("ab").count(), 2000);
    }
    eprintln!(
        "a183 C: longest line={longest_line}; adjacent pieces={}; source bytes per piece=4000",
        pieces.len()
    );
}
