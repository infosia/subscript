//! A conditional in a statement place takes `void` leaves (compiler.md
//! §258).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
function g(n: i32): void { print(`${n}`); }
function k(): i32 { return 7; }
function run(cb: (n: i32) => void): void { cb(1); }
type CB = (n: i32) => void;
class H { cb: CB | null = null; }
let b = true;
let n = 1;
"#;

/// The body starts on this line of the source.
const BODY_LINE: u32 = 9;

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

/// The diagnostics of a rejected body, as (code, message, line in the
/// body, column).
fn rejects(body: &str) -> Vec<(RuleCode, String, u32, u32)> {
    check(body)
        .expect_err(body)
        .into_iter()
        .map(|error| {
            (
                error.code,
                error.message,
                error.pos.line + 1 - BODY_LINE,
                error.pos.col,
            )
        })
        .collect()
}

const VOID_VALUE: &str = "a `void` expression is only allowed as an expression statement";
const VOID_RETURN: &str = "a `void` function cannot return a value";

/// Rules 1 and 2: each form of the first table of §258 is accepted, and
/// each control of the second table stays accepted.
#[test]
fn each_statement_form_and_control_is_accepted() {
    for body in [
        "function f(): void { b && g(1); }",
        "function f(): void { b || g(2); }",
        "function f(h: H): void { h.cb !== null && h.cb(1); }",
        "function f(): void { b ? g(1) : k(); }",
        r#"function f(): void { n > 1 ? g(1) : n > 0 ? g(2) : print("z"); }"#,
        "function f(): void { (n > 0 ? g(1) : g(2)); }",
        r#"const f = (n: i32) => n > 0 ? g(n) : print("e");"#,
        "function f(): void { run((n: i32) => n > 0 && g(n)); }",
        r#"const f = (n: i32): void => n > 0 ? g(n) : print("e");"#,
        r#"function f(): void { run((n: i32) => n > 0 ? g(n) : print("e")); }"#,
        r#"function f(): void { return n > 0 ? g(n) : print("e"); }"#,
    ] {
        if let Err(errors) = check(body) {
            panic!("{body}: {errors:?}");
        }
    }
}

/// Rule 3: with no context, a lambda body with a `void` leaf beside a
/// value leaf, and `&&` with a `void` right operand, stay rejected at the
/// `void` leaf.
#[test]
fn a_lambda_body_with_no_context_keeps_each_leaf_void() {
    assert_eq!(
        rejects("const f = (n: i32) => n > 0 ? g(n) : k();"),
        vec![(RuleCode::S100, VOID_VALUE.to_string(), 1, 31)]
    );
    assert_eq!(
        rejects("const f = (n: i32) => n > 0 && g(n);"),
        vec![(RuleCode::S100, VOID_VALUE.to_string(), 1, 32)]
    );
}

/// Rule 4: a `return` in a `void` function and a body of a lambda with
/// the annotation `void` keep §237.
#[test]
fn a_void_return_keeps_each_leaf_void() {
    assert_eq!(
        rejects("function f(): void { return b && g(1); }"),
        vec![(RuleCode::S100, VOID_VALUE.to_string(), 1, 34)]
    );
    assert_eq!(
        rejects("function f(): void { return b ? g(1) : k(); }"),
        vec![(RuleCode::S100, VOID_RETURN.to_string(), 1, 22)]
    );
    assert_eq!(
        rejects("const f = (n: i32): void => n > 0 ? g(n) : k();"),
        vec![(RuleCode::S100, VOID_RETURN.to_string(), 1, 29)]
    );
}
