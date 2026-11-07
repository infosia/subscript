//! A converted built-in callback keeps the inline lowering of the
//! identical type (compiler.md §235 rule 5c).
//!
//! Cost: the tests check and lower seven short programs to LIR, with no
//! native code and no external process.

use subscript_codegen::lir::lower_module;
use subscript_compiler::{check_program, SourceFile};

/// The debug text of the LIR of `main` for a program whose `main` has
/// `body`.
fn main_lir(body: &str) -> String {
    let source = format!(
        "class C {{ v: i32 = 1; }}\n\
         function show(x: C | null): void {{ print(x === null ? \"null\" : `${{x.v}}`); }}\n\
         export function main(): void {{\nconst cs: C[] = [new C()];\n{body}\n}}\n"
    );
    let module = check_program(&[SourceFile::entry("test.ts", source.as_str())])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    let lir = lower_module(&module).unwrap_or_else(|error| panic!("{error:?}"));
    let main = lir
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("main");
    format!("{main:?}")
}

/// Rule 5c: a lambda and a named function with a `C | null` parameter for
/// `C` elements keep the inline loop of the identical type, with no
/// indirect call. The inline loop is the only lowering that reads an
/// iterator in `main`; the other lowering passes the callback to the
/// runtime, which calls it indirectly.
#[test]
fn a_converted_for_each_callback_has_no_indirect_call() {
    for body in [
        "cs.forEach((x: C | null): void => { show(x); });",
        "cs.forEach(show);",
        // The identical type, the control of the same shape.
        "cs.forEach((x: C): void => { show(x); });",
    ] {
        let lir = main_lir(body);
        assert!(lir.contains("Iterator"), "{body}");
        assert!(!lir.contains("Indirect"), "{body}");
    }
    // The firing control: a function value goes to the runtime call.
    let lir = main_lir("const f: (x: C) => void = (x: C): void => { show(x); }; cs.forEach(f);");
    assert!(!lir.contains("Iterator"));
}

/// Rule 5c: an operand whose parameter type is its `| null` form passes
/// with no coercion, so the converted callback adds no value, and no root
/// slot, to the call of the identical type.
#[test]
fn a_converted_callback_adds_no_coercion() {
    let coercions = |body: &str| main_lir(body).matches("Coerce").count();
    assert_eq!(
        coercions("cs.forEach((x: C | null): void => { show(x); });"),
        coercions("cs.forEach((x: C): void => { show(x); });")
    );
    assert_eq!(
        coercions("cs.forEach(show);"),
        coercions("cs.forEach((x: C): void => { show(x); });")
    );
}
