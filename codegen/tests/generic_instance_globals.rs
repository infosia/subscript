//! Generic body controls lower and execute with complete module signatures (§138).
//! Cost: two interpreter executions and one checker rejection; no native program compiles.

use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::{check_program, RuleCode, SourceFile};

fn output(files: &[SourceFile]) -> String {
    let module = check_program(files).expect("valid generic body control");
    let lir = lower_module(&module).expect("generic body control LIR");
    String::from_utf8(interpret(&lir).expect("generic body control output")).expect("UTF-8 output")
}

#[test]
fn a_generic_function_called_from_a_later_function_resolves_globals() {
    let source = "function peek<T>(x: T): i32 { return m.v; } \
                  class Foo { v: i32 = 7; } const m: Foo = new Foo(); \
                  export function main(): void { print(`${peek<i32>(1)}`); }";
    assert_eq!(output(&[SourceFile::entry("main.ts", source)]), "7\n");
    let early = source.replace(
        "const m: Foo = new Foo();",
        "const early: i32 = peek<i32>(1); const m: Foo = new Foo();",
    );
    let errors = check_program(&[SourceFile::entry("main.ts", early)])
        .expect_err("the generic function reads an uninitialized global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "`m` is accessed before its declaration, through `peek<i32>`"
    );
}

#[test]
fn an_instance_in_an_import_signature_resolves_its_module_globals() {
    let files = [
        SourceFile::entry(
            "main.ts",
            "import { b } from './box'; export function main(): void { print(`${b.peek()}`); }",
        ),
        SourceFile::new(
            "box.ts",
            "import { m } from './value'; class Box<T> { peek(): i32 { return m.v; } } \
             export const b: Box<i32> = new Box<i32>();",
        ),
        SourceFile::new(
            "value.ts",
            "export class Foo { v: i32 = 7; } export const m: Foo = new Foo();",
        ),
    ];
    assert_eq!(output(&files), "7\n");
}
