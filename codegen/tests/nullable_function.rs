//! Nullable function construction, joins, and null tests (compiler.md §122).

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn corpus() -> [SourceFile; 1] {
    [SourceFile::new(
        "a271-nullable-function-value.ts",
        include_str!("../../corpus/accept/a271-nullable-function-value.ts"),
    )]
}

const EXPECTED: &[u8] = include_bytes!("../../corpus/accept/a271-nullable-function-value.expected");

#[test]
fn interpreter_builds_joins_and_tests_null_functions() {
    let module =
        lower_module(&check_program(&corpus()).expect("checked corpus")).expect("lowered corpus");
    assert_eq!(interpret(&module).expect("interpreter"), EXPECTED);
}

#[test]
fn jit_builds_joins_and_tests_null_functions() {
    assert_eq!(run_jit(&corpus()).expect("dev JIT"), EXPECTED);
}

#[test]
fn c_builds_joins_and_tests_null_functions() {
    assert_eq!(run_c_aot(&corpus()).expect("ship C"), EXPECTED);
}

#[test]
fn nullable_captures_survive_loop_joins_and_collection() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        class Box { value: i32 = 31; }
        function named(): i32 { return 9; }
        let global: (() => i32) | null = null;
        function readGlobal(): i32 {
            if (global === null) { return 0; }
            return global();
        }
        function joined(f: (() => i32) | null, empty: boolean): void {
            const missing: (() => i32) | null = null;
            let callback: (() => i32) | null = empty ? missing : f;
            let i: i32 = 0;
            while (i < 3) {
                Context.collect();
                print(`${callback === null} ${callback !== null ? callback() : -1}`);
                callback = i === 0 ? f : missing;
                i += 1;
            }
            const coalesced = f ?? missing;
            print(`${coalesced === null}`);
        }
        export function main(): void {
            print(`${readGlobal()}`);
            global = named;
            print(`${readGlobal()}`);
            joined(null, false);
            const box = new Box();
            const callback: (() => i32) | null = (): i32 => box.value;
            joined(callback, true);
        }
    "#,
    )];
    let expected = b"0\n9\ntrue -1\ntrue -1\ntrue -1\ntrue\ntrue -1\nfalse 31\ntrue -1\nfalse\n";
    let module =
        lower_module(&check_program(&files).expect("checked joins")).expect("lowered joins");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

#[test]
fn awaited_nullable_capture_keeps_its_environment() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        class Box { value: i32 = 41; }
        async function tick(): Promise<i32> { return 1; }
        async function call(f: (() => i32) | null): Promise<i32> {
            const callback: (() => i32) | null = f === null ? null : f;
            const step = await tick();
            Context.collect();
            return callback !== null ? callback() + step : 0;
        }
        export async function main(): Promise<void> {
            const box = new Box();
            const f: (() => i32) | null = (): i32 => box.value;
            print(`${await call(null)}`);
            print(`${await call(f)}`);
        }
    "#,
    )];
    let expected = b"0\n42\n";
    let module =
        lower_module(&check_program(&files).expect("checked await")).expect("lowered await");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}
