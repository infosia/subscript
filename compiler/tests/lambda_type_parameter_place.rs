//! A lambda in a type parameter place names no TypeScript form
//! (compiler.md §252).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
type CB = (x: i32) => void;
function wrap<T>(f: T): T { return f; }
class Box<T> { f: T; constructor(f: T) { this.f = f; } }
function nn<T>(f: T | null): void {}
function mk(): CB { return (x: i32): void => {}; }
function lone<T>(f: (x: T) => void): void {}
function each<T>(xs: T[], f: (x: T) => void): void {}
function fn<T>(f: ((x: T) => void) | null): void {}
"#;

const MESSAGE: &str = "parameters require a type annotation";

/// The diagnostics of a rejected body.
fn diagnostics(body: &str) -> Vec<Diagnostic> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).expect_err(body)
}

/// The one diagnostic of a rejected body.
fn rejects(body: &str) -> Diagnostic {
    let errors = diagnostics(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    errors.into_iter().next().expect("one diagnostic")
}

/// Rule 2: a lambda in a place whose declared type is a type parameter,
/// or `T | null`, has no context. The diagnostic is a `tsc` rejection with
/// no divergence block, also with explicit type arguments and in a list
/// that a hint reaches (`r497`). Rule 3: the message stays.
#[test]
fn a_type_parameter_place_gives_no_block() {
    for body in [
        "function f(): void { wrap((x) => print(`w ${x}`)); }",
        "function f(): void { const b = new Box((x) => print(`b ${x}`)); }",
        "function f(): void { nn((x) => print(`n ${x}`)); }",
        "function f(): void { const a = [mk, () => wrap((x) => print(`w ${x}`))]; }",
        "function f(): void { wrap<i32>((x) => print(`w ${x}`)); }",
        "function f(): void { const b = new Box<i32>((x) => print(`b ${x}`)); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, MESSAGE, "{body}");
        assert_eq!(diagnostic.divergence, None, "{body}");
    }
}

/// Rule 1: a lambda with more parameters than the function type `F` of
/// its place gives no block, in a call that infers its type arguments
/// and in a call with explicit type arguments. `tsc` gives TS2345 and
/// TS7006 on each form.
#[test]
fn a_lambda_with_more_parameters_gives_no_block() {
    for body in [
        "function f(): void { wrap<CB>((x, y) => {}); }",
        "function f(): void { const b = new Box<CB>((x, y) => {}); }",
        "function f(): void { lone((x, y) => {}); }",
        "function f(): void { lone<i32>((x, y) => {}); }",
    ] {
        let errors = diagnostics(body);
        assert!(
            errors.iter().all(|error| error.message == MESSAGE
                && error.code == RuleCode::S100
                && error.divergence.is_none()),
            "{body}: {errors:?}"
        );
    }
}

/// Rule 1: a lambda in a place whose type is a function type, or
/// `F | null`, keeps the divergence block of a generic callback: the
/// declared type in a call that infers its type arguments, and the type
/// after the substitution with explicit type arguments. A binding pattern
/// parameter takes no type from the context, so the explicit forms reach
/// the diagnostic. `tsc` accepts each form.
#[test]
fn a_function_type_place_keeps_the_block() {
    for body in [
        "function f(): void { lone((x) => {}); }",
        "function f(): void { each([1, 2], (x) => {}); }",
        "function f(): void { fn((x) => {}); }",
        "function f(): void { lone<i32[]>(([a]) => {}); }",
        "function f(): void { wrap<(x: i32[]) => void>(([a]) => {}); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, MESSAGE, "{body}");
        assert_eq!(
            diagnostic.divergence,
            Some(Divergence::GenericCallbackParameterAnnotationNeeded),
            "{body}"
        );
    }
}
