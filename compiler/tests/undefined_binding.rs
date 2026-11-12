//! A binding named `undefined` is a binding (compiler.md §253).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

fn accepts(body: &str) {
    check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
}

/// The one diagnostic of a rejected body.
fn rejects(body: &str) -> Diagnostic {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    errors.into_iter().next().expect("one diagnostic")
}

/// Rules 1, 2, and 4: with a binding named `undefined` in scope, a return
/// operand reads the binding, so a function with the result `void`
/// returns a value. `tsc` gives TS2322 on each form.
#[test]
fn a_return_reads_the_binding() {
    for body in [
        "function f(undefined: i32): void { return undefined; }",
        "function g(): void { const undefined = 3; return undefined; }",
        "const k = (undefined: i32): void => undefined;",
        "function h(undefined: i32): void { return (undefined); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message, "a `void` function cannot return a value",
            "{body}"
        );
    }
}

/// Rules 1 and 2: with a binding in scope, an operand, an absence
/// comparison, and a `for…of` binding read the binding. `tsc` accepts each
/// form.
#[test]
fn each_other_place_reads_the_binding() {
    for body in [
        "function n(undefined: i32): i32 { return undefined + 1; }",
        "function a(undefined: string | null): boolean { return undefined === null; }",
        "function q(): void { for (const undefined of [1, 2]) { print(`q ${undefined}`); } }",
    ] {
        accepts(body);
    }
}

/// Rule 3 and the controls: with no binding, `return undefined;` in a
/// function with the result `void` stays accepted, and the token stays
/// rejected where C7 rejects it. A binding in an inner scope does not
/// change the token in an outer scope, or in a later function.
#[test]
fn the_token_with_no_binding_does_not_change() {
    accepts("function f(): void { return undefined; }");
    accepts(
        "function f(b: boolean): void { if (b) { const undefined = 3; print(`${undefined}`); } return undefined; }",
    );
    accepts("function f(undefined: i32): void { print(`${undefined}`); }\nfunction g(): void { return undefined; }");
    for body in [
        "function f(): void { const x = undefined; }",
        "function f(b: boolean): void { if (b) { const undefined = 3; print(`${undefined}`); } const x = undefined; }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S012, "{body}: {diagnostic:?}");
    }
}
