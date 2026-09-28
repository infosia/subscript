//! Cost: each case compiles one C program and runs all three engines.
use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::lower_module,
    run_c_aot, run_jit, RunError,
};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::TrapKind;

fn agree(source: &str, expected: &[u8]) {
    let files = [SourceFile::new("test.ts", source)];
    let hir = check_program(&files).unwrap();
    let lir = lower_module(&hir).unwrap();
    assert_eq!(interpret(&lir).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn array_search_corpus_matches_three_engines() {
    agree(
        include_str!("../../corpus/accept/a268-array-string-at-find-flatmap.ts"),
        include_bytes!("../../corpus/accept/a268-array-string-at-find-flatmap.expected"),
    );
}

#[test]
fn signed_index_traps_keep_kind_message_and_position() {
    for (expression, kind, message) in [
        ("[1, 2, 3].at(3)", TrapKind::IndexOutOfBounds, ""),
        ("[1, 2, 3].at(-4)", TrapKind::IndexOutOfBounds, ""),
        ("empty.at(0)", TrapKind::IndexOutOfBounds, ""),
        ("functions.at(0)", TrapKind::IndexOutOfBounds, ""),
        (
            "\"abc\".at(3)",
            TrapKind::StrRange,
            "codePointAt(3) out of range for string length 3",
        ),
        (
            "\"abc\".at(-4)",
            TrapKind::StrRange,
            "codePointAt(-1) out of range for string length 3",
        ),
        (
            "\"héllo\".at(2)",
            TrapKind::StrRange,
            "charAt(2) is not on a UTF-8 boundary",
        ),
        (
            "\"héllo\".at(-4)",
            TrapKind::StrRange,
            "charAt(2) is not on a UTF-8 boundary",
        ),
    ] {
        let source = format!(
            "export function main(): void {{ const empty: i32[] = []; const functions: (() => i32)[] = [];\n  {expression};\n}}"
        );
        let files = [SourceFile::new("test.ts", source)];
        let Err(RunError::Trap(jit)) = run_jit(&files) else {
            panic!("expected JIT trap: {expression}");
        };
        let Err(RunError::Trap(ship)) = run_c_aot(&files) else {
            panic!("expected C trap: {expression}");
        };
        assert_eq!(jit, ship);
        assert_eq!(jit.rule, kind);
        assert_eq!((jit.pos.line, jit.pos.col), (2, 3));
        if !message.is_empty() {
            assert_eq!(jit.message, message);
        }
        let hir = check_program(&files).unwrap();
        let lir = lower_module(&hir).unwrap();
        let Err(InterpretError::Execution { source, .. }) = interpret(&lir) else {
            panic!("expected interpreter trap");
        };
        let InterpretError::Trap {
            kind, pos, message, ..
        } = *source
        else {
            panic!("expected interpreter trap detail");
        };
        assert_eq!(kind, jit.rule.rule());
        assert_eq!(pos, jit.pos);
        assert_eq!(message, jit.message);
    }
}

#[test]
fn callbacks_keep_order_original_values_null_misses_and_depth_one() {
    agree(
        r#"
class C { v: i32; constructor(v: i32) { this.v = v; } }
function forward(cs: C[], f: (c: C, i: i32) => boolean): C | null { return cs.find(f); }
function backward(cs: C[], f: (c: C, i: i32) => boolean): C | null { return cs.findLast(f); }
function index(cs: C[], f: (c: C, i: i32) => boolean): i32 { return cs.findLastIndex(f); }
export function main(): void {
  const cs: C[] = [new C(1), new C(2), new C(3)];
  print(`${cs.find((c: C): boolean => false) === null} ${cs.findLast((c: C): boolean => false) === null}`);
  const order: i32[] = [0];
  const found = backward(cs, (c: C, i: i32): boolean => { order[0] = order[0] * 10 + i; cs[i] = new C(9); return i === 1; });
  print(`${order[0]} ${(found ?? new C(-1)).v} ${cs[1].v}`);
  print(`${(forward(cs, (c: C, i: i32): boolean => i === 0) ?? new C(-1)).v}`);
  print(`${index(cs, (c: C, i: i32): boolean => i === 1)}`);
  const shared: i32[] = [0];
  const flat = [1, 2].flatMap((v: i32): i32[] => { shared[0] = v; return shared; });
  print(flat.join(","));
  const deep: i32[][][] = [[[1, 2]], [[3]]];
  const shallow = deep.flatMap((v: i32[][]): i32[][] => v);
  print(`${shallow.length} ${shallow[0].join(",")} ${shallow[1].join(",")}`);
  const refs: C[][] = [cs];
  print(`${(refs.find((v: C[]): boolean => true) ?? []).length}`);
  print(`${refs.findLast((v: C[]): boolean => false) === null}`);
  try { cs.findLast((c: C): boolean => { throw new Error("predicate"); }); }
  catch (e) { if (e instanceof Error) { print(e.message); } }
  try { [1].flatMap((v: i32): i32[] => { throw new Error("mapping"); }); }
  catch (e) { if (e instanceof Error) { print(e.message); } }
}
"#,
        b"true true\n21 2 9\n1\n1\n1,2\n2 1,2 3\n3\ntrue\npredicate\nmapping\n",
    );
}

#[test]
fn at_and_flat_map_preserve_aggregate_and_function_elements() {
    agree(
        r#"
@CStruct
class Pair { x: i32; y: f64; constructor(x: i32, y: f64) { this.x = x; this.y = y; } }
function seven(): i32 { return 7; }
export function main(): void {
  const pairs: Pair[] = [new Pair(1, 2.5), new Pair(3, 4.5)];
  print(`${pairs.at(-1).x} ${pairs.at(0).y}`);
  const flat: Pair[] = [1, 2].flatMap((v: i32): Pair[] => [new Pair(v, 2.5)]);
  print(`${flat.length} ${flat[0].x} ${flat[1].y}`);
  const fns: (() => i32)[] = [seven];
  const f = fns.at(-1);
  print(`${f()}`);
  const functions = [1, 2].flatMap((v: i32): (() => i32)[] => [seven]);
  const g = functions[1];
  print(`${functions.length} ${g()}`);
  print(`${[true, false].at(-1)} ${[true, false].flatMap((v: boolean): boolean[] => [v, v]).join(",")}`);
}
"#,
        b"3 2.5\n2 1 2.5\n7\n2 7\nfalse true,true,false,false\n",
    );
}
