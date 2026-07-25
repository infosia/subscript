#![warn(missing_docs)]
//! HIR-to-CLIF lowering and both execution tiers of subscript (plan
//! phases P2 and P3, `specs/blocks/compiler.md` §7, §8).
//!
//! One lowering serves both tiers (§1): the `lower` module targets
//! the `cranelift_module::Module` trait, the dev-tier JIT driver
//! instantiates it with `JITModule`, and the ship-tier AOT driver
//! instantiates the same lowering with `ObjectModule`.
//!
//! Three entry points, all returning the exact stdout bytes of a run
//! (print writes to a runtime-owned sink, never the process stdout) or
//! a [`TrapReport`]:
//!
//! - [`run_jit`] — dev tier: check, lower, execute the exported
//!   `main(): void` in process.
//! - [`run_aot`] — ship tier: check, lower, emit an object, link it
//!   with the runtime static library and the generated C entry, run
//!   the binary. [`emit_object`] stops after emission and is what the
//!   device-triple link script uses.
//! - [`ReloadSession`] — dev tier with hot reload: a live program whose
//!   function bodies can be swapped between host calls, accepted only
//!   when its [`DeclarationHash`] is unchanged.
//!
//! The standing differential gate (§8.3) compares the first two
//! against the committed goldens on every `cargo test`.
//!
//! [`jit_bench`] is the dev tier's measurement entry point for the P4
//! performance gate (§9): same compilation, but the exported `main` is
//! called repeatedly and each call is timed on its own.

mod aot;
mod cemit;
mod jit;
mod layout;
mod lower;
mod reload;

pub use aot::{
    emit_object, run_aot, run_c_aot, runtime_staticlib_path, AotObject, AOT_ENTRY_C,
    RUNTIME_STATICLIB_ENV,
};
pub use cemit::{emit_c, CProgram};
pub use jit::{jit_bench, run_jit, BenchSamples, RunError, TrapReport};
pub use layout::{value_class_layouts, FieldLayout, StructLayout};
pub use reload::{declaration_hash, DeclarationHash, ReloadError, ReloadSession};

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::SourceFile;
    use subscript_runtime::TrapKind;

    fn run(src: &str) -> Result<Vec<u8>, RunError> {
        run_jit(&[SourceFile::new("test.ts", src)])
    }

    fn run_ok(src: &str) -> String {
        match run(src) {
            Ok(bytes) => String::from_utf8(bytes).expect("utf-8 output"),
            Err(e) => panic!("run failed: {e}"),
        }
    }

    fn run_trap(src: &str) -> TrapReport {
        match run(src) {
            Err(RunError::Trap(t)) => t,
            Ok(out) => panic!("expected a trap, got output {:?}", String::from_utf8_lossy(&out)),
            Err(e) => panic!("expected a trap, got {e}"),
        }
    }

    #[test]
    fn date_field_codes_agree_between_compiler_and_runtime() {
        // The sub_rt_date_get contract: the checker/codegen field codes
        // (hir::DateFn::field_code) and the runtime's FIELD_* constants
        // are one table. This crate sees both crates, so it holds the
        // cross-check.
        use subscript_compiler::hir::DateFn;
        use subscript_runtime::date;
        let pairs = [
            (DateFn::GetUtcFullYear, date::FIELD_FULL_YEAR),
            (DateFn::GetUtcMonth, date::FIELD_MONTH),
            (DateFn::GetUtcDate, date::FIELD_DATE),
            (DateFn::GetUtcDay, date::FIELD_DAY),
            (DateFn::GetUtcHours, date::FIELD_HOURS),
            (DateFn::GetUtcMinutes, date::FIELD_MINUTES),
            (DateFn::GetUtcSeconds, date::FIELD_SECONDS),
            (DateFn::GetUtcMilliseconds, date::FIELD_MILLISECONDS),
        ];
        for (f, code) in pairs {
            assert_eq!(f.field_code(), Some(code), "field code of {}", f.name());
        }
    }

    #[test]
    fn arr_kind_codes_agree_between_compiler_and_runtime() {
        // The sub_rt_arr_* tag contract (stdlib.md §9): the compiler's
        // ArrElemKind/ArrFmtKind codes and the runtime arrops decoders
        // are one table. This crate sees both crates, so it holds the
        // cross-check.
        use subscript_compiler::hir::{ArrElemKind, ArrFmtKind};
        use subscript_runtime::arrops::{ElemKind, FmtKind};
        for (compiler, runtime) in [
            (ArrElemKind::Int, ElemKind::Int),
            (ArrElemKind::F32, ElemKind::F32),
            (ArrElemKind::F64, ElemKind::F64),
            (ArrElemKind::Str, ElemKind::Str),
        ] {
            assert_eq!(
                ElemKind::from_u32(compiler.code()),
                Some(runtime),
                "elem kind {compiler:?}"
            );
        }
        for (compiler, runtime) in [
            (ArrFmtKind::I32, FmtKind::I32),
            (ArrFmtKind::U32, FmtKind::U32),
            (ArrFmtKind::I64, FmtKind::I64),
            (ArrFmtKind::U64, FmtKind::U64),
            (ArrFmtKind::F32, FmtKind::F32),
            (ArrFmtKind::F64, FmtKind::F64),
            (ArrFmtKind::Bool, FmtKind::Bool),
            (ArrFmtKind::Str, FmtKind::Str),
        ] {
            assert_eq!(
                FmtKind::from_u32(compiler.code()),
                Some(runtime),
                "format kind {compiler:?}"
            );
        }
    }

    #[test]
    fn hello_prints_to_the_sink() {
        assert_eq!(run_ok("export function main(): void {\n  print(\"hello\");\n}\n"), "hello\n");
    }

    #[test]
    fn rejected_programs_surface_diagnostics() {
        let err = run("const x: number = 1;\n");
        assert!(matches!(err, Err(RunError::Rejected(_))));
    }

    #[test]
    fn sized_arithmetic_and_q14_formatting() {
        let out = run_ok(
            "export function main(): void {\n  const a: i32 = -7;\n  const b: u32 = 4294967295;\n  const c: i64 = 1;\n  let d: u64 = 0;\n  d -= 1;\n  print(`${a},${b},${c - 2},${d}`);\n}\n",
        );
        // The u64 literal surface caps at 2^53-1, so u64::MAX is built
        // by wraparound (two's complement per C3).
        assert_eq!(out, "-7,4294967295,-1,18446744073709551615\n");
    }

    #[test]
    fn q18_bitwise_is_true_64_bit() {
        let out = run_ok(
            "export function main(): void {\n  const one: u64 = 1;\n  const high: u64 = one << 40;\n  const mixed: u64 = high | 255;\n  print(`${high},${mixed},${~one}`);\n}\n",
        );
        assert_eq!(out, "1099511627776,1099511628031,18446744073709551614\n");
    }

    #[test]
    fn f32_arithmetic_stays_in_f32() {
        // 16777216f32 + 1 == 16777216 in f32; computing in f64 and
        // rounding would give 16777217 (a22's checksum depends on
        // true f32 arithmetic).
        let out = run_ok(
            "export function main(): void {\n  const big: f32 = 16777216.0;\n  const one: f32 = 1.0;\n  print(`${big + one}`);\n}\n",
        );
        assert_eq!(out, "16777216\n");
    }

    #[test]
    fn division_by_zero_traps_with_position() {
        let t = run_trap(
            "function f(d: i32): i32 {\n  return 10 / d;\n}\nexport function main(): void {\n  print(`${f(0)}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::DivisionByZero);
        assert_eq!(t.pos.file, "test.ts");
        assert_eq!(t.pos.line, 2);
    }

    #[test]
    fn signed_division_min_by_minus_one_wraps() {
        let out = run_ok(
            "export function main(): void {\n  let x: i32 = -2147483648;\n  let d: i32 = -1;\n  print(`${x / d},${x % d}`);\n}\n",
        );
        assert_eq!(out, "-2147483648,0\n");
    }

    #[test]
    fn array_oob_traps_at_the_index_expression() {
        let t = run_trap(
            "export function main(): void {\n  const xs: i32[] = [1, 2];\n  print(`${xs[5]}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
        assert_eq!(t.pos.line, 3);
    }

    #[test]
    fn empty_pop_traps() {
        let t = run_trap(
            "export function main(): void {\n  const xs: i32[] = [];\n  xs.pop();\n}\n",
        );
        assert_eq!(t.rule, TrapKind::EmptyPop);
    }

    #[test]
    fn fixed_array_oob_traps() {
        let t = run_trap(
            "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  let i: i32 = 7;\n  print(`${xs[i]}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
    }

    #[test]
    fn string_slice_off_boundary_traps() {
        let t = run_trap(
            "export function main(): void {\n  const s: string = \"h\\u00e9llo\";\n  print(s.slice(0, 2));\n}\n",
        );
        assert_eq!(t.rule, TrapKind::StringSlice);
        assert_eq!(t.pos.line, 3);
    }

    #[test]
    fn double_delete_traps() {
        let t = run_trap(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  unsafeDelete(c);\n  unsafeDelete(c);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::DoubleDelete);
        assert_eq!(t.pos.line, 5);
    }

    #[test]
    fn use_after_delete_traps() {
        let t = run_trap(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  unsafeDelete(c);\n  print(`${c.x}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::UseAfterDelete);
        assert_eq!(t.pos.line, 5);
    }

    #[test]
    fn value_class_copies_on_assign_and_pass() {
        let out = run_ok(
            "@CStruct\nclass V { x: i32; constructor(x: i32) { this.x = x; } }\nfunction bump(v: V): i32 {\n  v.x += 100;\n  return v.x;\n}\nexport function main(): void {\n  const a: V = new V(1);\n  const b: V = a;\n  b.x = 9;\n  print(`${a.x},${b.x},${bump(a)},${a.x}`);\n}\n",
        );
        assert_eq!(out, "1,9,101,1\n");
    }

    #[test]
    fn default_parameters_fill_at_the_call_site() {
        let out = run_ok(
            "function scale(v: i32, k: i32 = 3): i32 {\n  return v * k;\n}\nexport function main(): void {\n  print(`${scale(7)},${scale(7, 4)}`);\n}\n",
        );
        assert_eq!(out, "21,28\n");
    }

    #[test]
    fn function_pointers_and_capturing_lambdas() {
        let out = run_ok(
            "function inc(x: i32): i32 {\n  return x + 1;\n}\nfunction apply(f: (x: i32) => i32, v: i32): i32 {\n  return f(v);\n}\nexport function main(): void {\n  const k: i32 = 5;\n  const addK: (x: i32) => i32 = (x: i32): i32 => x + k;\n  print(`${apply(inc, 8)},${apply(addK, 8)}`);\n}\n",
        );
        assert_eq!(out, "9,13\n");
    }

    #[test]
    fn generator_drives_to_done_with_zeroed_value() {
        let out = run_ok(
            "function* seq(limit: i32) {\n  for (let v: i32 = 1; v <= limit; v += 1) {\n    yield v;\n  }\n}\nexport function main(): void {\n  const g = seq(3);\n  let total: i32 = 0;\n  let steps: i32 = 0;\n  while (true) {\n    const s = g.next();\n    if (s.done) {\n      total += s.value;\n      break;\n    }\n    total += s.value;\n    steps += 1;\n  }\n  const t = g.next();\n  print(`${total},${steps},${t.done},${t.value}`);\n}\n",
        );
        // 1+2+3 plus a zeroed value at done; done stays done.
        assert_eq!(out, "6,3,true,0\n");
    }

    #[test]
    fn generator_frame_offsets_survive_dead_code_lets() {
        // `let`s after a terminator are counted by the frame pre-pass
        // but never lowered; the offset cursor must stay aligned.
        let out = run_ok(
            "function* seq() {\n  let i: i32 = 0;\n  while (i < 2) {\n    yield i;\n    i += 1;\n    continue;\n    const deadA: i64 = 9;\n    const deadB: i32 = 1;\n  }\n  const tail: i32 = 40 + i;\n  yield tail;\n}\nexport function main(): void {\n  const g = seq();\n  let total: i32 = 0;\n  while (true) {\n    const s = g.next();\n    if (s.done) {\n      break;\n    }\n    total += s.value;\n  }\n  print(`${total}`);\n}\n",
        );
        // 0 + 1 + (40 + 2)
        assert_eq!(out, "43\n");
    }

    #[test]
    fn collect_after_dropping_the_last_reference_is_safe() {
        let out = run_ok(
            "class T { id: i32; constructor(id: i32) { this.id = id; } }\nexport function main(): void {\n  let t: T | null = new T(17);\n  if (t !== null) {\n    print(`${t.id}`);\n  }\n  t = null;\n  collect();\n  print(\"ok\");\n}\n",
        );
        assert_eq!(out, "17\nok\n");
    }

    #[test]
    fn collect_keeps_rooted_locals_alive() {
        let out = run_ok(
            "class T { id: i32; constructor(id: i32) { this.id = id; } }\nexport function main(): void {\n  const t: T = new T(5);\n  collect();\n  print(`${t.id}`);\n}\n",
        );
        assert_eq!(out, "5\n");
    }

    #[test]
    fn string_equality_is_by_content() {
        let out = run_ok(
            "export function main(): void {\n  const a: string = \"al\" + \"pha\";\n  const b: string = \"alpha\";\n  if (a === b) {\n    print(\"same\");\n  }\n  if (a !== \"beta\") {\n    print(\"diff\");\n  }\n}\n",
        );
        assert_eq!(out, "same\ndiff\n");
    }

    #[test]
    fn two_file_programs_link_across_modules() {
        let out = run_jit(&[
            SourceFile::new(
                "main.ts",
                "import { double } from \"./util\";\nexport function main(): void {\n  print(`${double(21)}`);\n}\n",
            ),
            SourceFile::new(
                "util.ts",
                "export function double(x: i32): i32 {\n  return x * 2;\n}\n",
            ),
        ])
        .expect("two-file run");
        assert_eq!(out, b"42\n");
    }

    #[test]
    fn global_state_persists_across_calls() {
        let out = run_ok(
            "let counter: i32 = 10;\nfunction bump(): i32 {\n  counter += 1;\n  return counter;\n}\nexport function main(): void {\n  print(`${bump()},${bump()},${counter}`);\n}\n",
        );
        assert_eq!(out, "11,12,12\n");
    }

    // ----- P2 phase-review regression tests -----

    #[test]
    fn m1_reference_held_only_in_a_fixed_array_survives_collect() {
        // GC root coverage: the only handle to the C instance lives
        // inside a FixedArray local; collect() must not free it.
        let out = run_ok(
            "class C { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const xs: FixedArray<C, 1> = [new C(7)];\n  collect();\n  print(`${xs[0].x}`);\n}\n",
        );
        assert_eq!(out, "7\n");
    }

    #[test]
    fn m1_string_held_only_in_an_aggregate_survives_collect() {
        // The concatenation result is not interned; the FixedArray
        // interior is its only reference.
        let out = run_ok(
            "export function main(): void {\n  const parts: FixedArray<string, 2> = [\"al\" + \"pha\", \"beta\"];\n  collect();\n  print(parts[0] + parts[1]);\n}\n",
        );
        assert_eq!(out, "alphabeta\n");
    }

    #[test]
    fn m1_iter_result_string_survives_collect() {
        let out = run_ok(
            "function* words() {\n  yield \"al\" + \"pha\";\n}\nexport function main(): void {\n  const g = words();\n  const s = g.next();\n  collect();\n  print(s.value);\n}\n",
        );
        assert_eq!(out, "alpha\n");
    }

    #[test]
    fn m1_aggregate_params_are_rooted_in_the_callee() {
        // The caller's argument temp is not a root; the callee must
        // root its own copy before running collect().
        let out = run_ok(
            "class C { x: i32; constructor(x: i32) { this.x = x; } }\nfunction probe(xs: FixedArray<C, 1>): i32 {\n  collect();\n  return xs[0].x;\n}\nexport function main(): void {\n  print(`${probe([new C(9)])}`);\n}\n",
        );
        assert_eq!(out, "9\n");
    }

    #[test]
    fn m2_generator_with_unreachable_yield_after_continue() {
        let out = run_ok(
            "function* seq() {\n  let i: i32 = 0;\n  while (i < 2) {\n    yield i;\n    i += 1;\n    continue;\n    yield 99;\n  }\n}\nexport function main(): void {\n  const g = seq();\n  let total: i32 = 0;\n  while (true) {\n    const s = g.next();\n    if (s.done) {\n      break;\n    }\n    total += s.value;\n  }\n  print(`${total}`);\n}\n",
        );
        assert_eq!(out, "1\n");
    }

    #[test]
    fn m2_unreachable_yield_inside_dead_if_arm_after_return() {
        let out = run_ok(
            "function* seq() {\n  yield 4;\n  return;\n  if (true) {\n    yield 99;\n  }\n}\nexport function main(): void {\n  const g = seq();\n  let total: i32 = 0;\n  while (true) {\n    const s = g.next();\n    if (s.done) {\n      break;\n    }\n    total += s.value;\n  }\n  print(`${total}`);\n}\n",
        );
        assert_eq!(out, "4\n");
    }

    #[test]
    fn n3_assignment_rhs_that_grows_the_array_is_not_lost() {
        // xs starts at len 4 == capacity; grow() reallocates the
        // element storage, so the element address must be resolved
        // after the RHS runs.
        let out = run_ok(
            "let xs: i32[] = [];\nfunction grow(): i32 {\n  for (let i: i32 = 0; i < 60; i += 1) {\n    xs.push(0);\n  }\n  return 42;\n}\nexport function main(): void {\n  xs.push(1);\n  xs.push(2);\n  xs.push(3);\n  xs.push(4);\n  xs[0] = grow();\n  print(`${xs[0]},${xs.length}`);\n}\n",
        );
        assert_eq!(out, "42,64\n");
    }

    #[test]
    fn n4_trap_inside_a_generator_unwinds_to_a_report() {
        // The resume unwind path (which now also stores the terminal
        // state) must hand the trap back through `.next()` and `main`.
        let t = run_trap(
            "function* bad() {\n  const xs: i32[] = [1];\n  yield xs[5];\n}\nexport function main(): void {\n  const g = bad();\n  const s = g.next();\n  print(`${s.value}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
        assert_eq!(t.pos.line, 3);
    }

    #[test]
    fn n2_forward_nested_value_classes_run_correctly() {
        // Outer embeds Inner declared after it; the layout must be
        // computed by resolving the forward reference.
        let out = run_ok(
            "@CStruct\nclass Outer { inner: Inner; pad: f32;\n  constructor(inner: Inner, pad: f32) { this.inner = inner; this.pad = pad; }\n}\n@CStruct\nclass Inner { x: f64;\n  constructor(x: f64) { this.x = x; }\n}\nexport function main(): void {\n  const o: Outer = new Outer(new Inner(2.5), 1.0);\n  print(`${o.inner.x},${o.pad}`);\n}\n",
        );
        assert_eq!(out, "2.5,1\n");
    }

    #[test]
    fn n2_value_class_cycle_is_an_internal_error_not_a_crash() {
        let err = run(
            "@CStruct\nclass S { s: S;\n  constructor(s: S) { this.s = s; }\n}\nexport function main(): void {}\n",
        );
        match err {
            Err(RunError::Internal(msg)) => assert!(msg.contains("cycle"), "got: {msg}"),
            other => panic!("expected an internal error, got {other:?}"),
        }
    }

    #[test]
    fn nan_and_infinity_spellings() {
        let out = run_ok(
            "export function main(): void {\n  const zero: f64 = 0.0;\n  const one: f64 = 1.0;\n  print(`${zero / zero},${one / zero},${-one / zero},${-zero}`);\n}\n",
        );
        assert_eq!(out, "NaN,Infinity,-Infinity,-0\n");
    }

    // ----- P4.1 bounds-check elimination: safety net (compiler.md §10) -----
    //
    // Proven-in-range FixedArray indices lose their check; every index
    // the analysis cannot prove must still trap at its TS position.

    #[test]
    fn proven_induction_indices_compute_the_right_sum() {
        // Nested counted loops with `r*4+c` in [0,15]: every access is
        // proven and elided, and the result must still be correct
        // (0+1+...+15 = 120).
        let out = run_ok(
            "export function main(): void {\n  const m: FixedArray<i32, 16> = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];\n  let sum: i32 = 0;\n  for (let r: i32 = 0; r < 4; r += 1) {\n    for (let c: i32 = 0; c < 4; c += 1) {\n      sum += m[r * 4 + c];\n    }\n  }\n  print(`${sum}`);\n}\n",
        );
        assert_eq!(out, "120\n");
    }

    #[test]
    fn proven_last_element_is_elided_and_correct() {
        // Boundary: the counter reaches exactly the last valid index
        // (n-1); the check is elided and the last element is read.
        let out = run_ok(
            "export function main(): void {\n  const xs: FixedArray<i32, 4> = [10, 20, 30, 40];\n  let sum: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) {\n    sum += xs[i];\n  }\n  print(`${sum}`);\n}\n",
        );
        assert_eq!(out, "100\n");
    }

    #[test]
    fn dynamic_index_from_argument_still_traps() {
        // The index is a parameter — unprovable — so the check stays and
        // fires at the indexing expression's position.
        let t = run_trap(
            "function at(xs: FixedArray<i32, 3>, i: i32): i32 {\n  return xs[i];\n}\nexport function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${at(xs, 5)}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
        assert_eq!(t.pos.line, 2);
    }

    #[test]
    fn loop_with_runtime_bound_still_traps() {
        // The loop bound is a parameter, so the counter's range is not
        // proven and the check stays; it fires when i reaches 3.
        let t = run_trap(
            "function fill(xs: FixedArray<i32, 3>, n: i32): i32 {\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < n; i += 1) {\n    total += xs[i];\n  }\n  return total;\n}\nexport function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${fill(xs, 5)}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
        assert_eq!(t.pos.line, 4);
    }

    #[test]
    fn loop_counter_from_negative_start_still_traps() {
        // A counter proven to be [-1, 2] is not in [0, n): the check
        // stays and fires on the first (i == -1) iteration.
        let t = run_trap(
            "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  let sink: i32 = 0;\n  for (let i: i32 = -1; i < 3; i += 1) {\n    sink += xs[i];\n  }\n  print(`${sink}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
        assert_eq!(t.pos.line, 5);
    }

    #[test]
    fn reassigned_counter_is_not_proven_and_still_traps() {
        // The body mutates the counter outside the step, so it is no
        // longer monotonic: the analysis must decline the proof and keep
        // the check, which fires when the mutated index leaves range.
        // Without the reassignment guard the counter would be wrongly
        // proven [0,2] and the out-of-range read would go unchecked.
        let t = run_trap(
            "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  let out: i32 = 0;\n  for (let i: i32 = 0; i < 3; i += 1) {\n    i += 5;\n    out += xs[i];\n  }\n  print(`${out}`);\n}\n",
        );
        assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
        assert_eq!(t.pos.line, 6);
    }

    #[test]
    fn constant_out_of_range_index_stays_a_checker_error() {
        // A constant literal index equal to the length is a checker
        // error (P1, S100), and remains so — the bounds-check elimination
        // must never turn a rejected program into an unchecked access.
        // Division of responsibility: constant OOB is P1's; every
        // non-constant OOB the analysis cannot prove is a runtime trap
        // (the tests above).
        let err = run(
            "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${xs[3]}`);\n}\n",
        );
        assert!(
            matches!(err, Err(RunError::Rejected(_))),
            "expected a checker rejection, got {err:?}"
        );
    }
}
