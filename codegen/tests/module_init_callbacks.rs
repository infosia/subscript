//! Direct built-in callbacks under compiler.md §137 rule 5b.
//! Cost: warm debug test execution 0.39 s, four ship-C program compiles.

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot_with_native_libraries,
    run_jit_with_native_libraries,
};
use subscript_compiler::{check_program, RuleCode, SourceFile};

fn checked_routes(source: &str, control: &str, global: &str, expected: &[u8]) {
    let errors = check_program(&[SourceFile::entry("main.ts", control)])
        .expect_err("callback reads a later global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        format!("`{global}` is accessed before its declaration, through a lambda")
    );
    let files = [SourceFile::entry("main.ts", source)];
    let module =
        check_program(&files).expect("the literal callback does not call the stored function");
    assert_eq!(
        interpret(&lower_module(&module).expect("LIR")).expect("interpreter"),
        expected
    );
    assert_eq!(
        run_jit_with_native_libraries(&files, &[]).expect("JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &[]).expect("ship C"),
        expected
    );
}

#[test]
fn a_for_each_literal_does_not_run_an_earlier_greeter() {
    let source = "function greet(n: string): void { print(`${greeting}, ${n}`); } const greeter: (n: string) => void = greet; const names: string[] = ['b', 'a']; names.forEach((n: string): void => { print(n); }); const greeting: string = 'hi'; export function main(): void { greeter(names[0]); }";
    let control = source.replace("print(n);", "print(greeting);");
    checked_routes(source, &control, "greeting", b"b\na\nhi, b\n");
}

#[test]
fn a_sort_literal_does_not_run_an_earlier_ready_callback() {
    let source = "class Registry { items: i32[] = [7]; } const onReady: (() => void)[] = []; onReady.push((): void => { print(`${registry.items.length}`); }); const ids: i32[] = [3, 1, 2]; ids.sort((a: i32, b: i32): i32 => a - b); const registry: Registry = new Registry(); export function main(): void { print(`${ids[0]}`); onReady[0](); }";
    let control = source.replace("=> a - b", "=> a - b + registry.items.length");
    checked_routes(source, &control, "registry", b"1\n1\n");
}

#[test]
fn a_for_each_literal_does_not_run_an_earlier_record_function() {
    let source = "class Stats { count: i32 = 8; } function record(): void { print(`${stats.count}`); } const track: () => void = record; const values: i32[] = [1, 2, 3]; let sum: i32 = 0; values.forEach((v: i32): void => { sum += v; }); const stats: Stats = new Stats(); export function main(): void { print(`${sum}`); track(); }";
    let control = source.replace("sum += v;", "sum += v + stats.count;");
    checked_routes(source, &control, "stats", b"6\n8\n");
}

#[test]
fn a_sort_literal_does_not_run_an_earlier_logger() {
    let source = "function log(s: string): void { print(`${prefix}: ${s}`); } const logger: (s: string) => void = log; const sorted: i32[] = [3, 1, 2]; sorted.sort((a: i32, b: i32): i32 => a - b); const prefix: string = 'app'; export function main(): void { logger(`${sorted[0]}`); }";
    let control = source.replace("=> a - b", "=> a - b + prefix.length");
    checked_routes(source, &control, "prefix", b"app: 1\n");
}
