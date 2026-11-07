//! A nested then takes the context of its adoption (compiler.md §245).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
type Mode = "a" | "b";
async function v(n: i32): Promise<i32> { return n; }
async function bad(n: i32): Promise<i32> { throw new Error("bad"); }
"#;

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source =
        format!("{DECLARATIONS}{body}\nexport async function main(): Promise<void> {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
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

/// Rule 1: a `then` body that is a `then` call takes `Promise<C>`, at two
/// and three levels, in a return value and an initializer, and through
/// parentheses.
#[test]
fn a_nested_then_takes_the_context_of_its_adoption() {
    for body in [
        "async function h(): Promise<Mode> { return v(1).then((x: i32) => v(x).then((y: i32) => \"a\")); }",
        "async function h(): Promise<void> { const p: Promise<Mode> = v(1).then((x: i32) => v(x).then((y: i32) => \"b\")); await p; }",
        "async function h(): Promise<Mode> { return v(1).then((x: i32) => v(x).then((y: i32) => v(y).then((z: i32) => \"a\"))); }",
        "async function h(): Promise<Mode> { return v(1).then((x: i32) => (v(x).then((y: i32) => \"a\"))); }",
    ] {
        accepts(body);
    }
}

/// Rules 1 and 3: a literal that is not a member of `Mode` stays rejected,
/// with the same message as before this section. The accepted case beside
/// it, a member of `Mode` in the same nesting, was rejected before this
/// section, so it shows that the context reaches the inner callback.
#[test]
fn a_nested_literal_outside_the_context_is_rejected() {
    for body in [
        "async function h(): Promise<Mode> { return v(1).then((x: i32) => v(x).then((y: i32) => \"c\")); }",
        "async function h(): Promise<Mode> { return v(1).then((x: i32) => v(x).then((y: i32) => v(y).then((z: i32) => \"c\"))); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message,
            "type mismatch: the return value expects `Mode`, got `Promise<string>`",
            "{body}"
        );
    }
    accepts("async function h(): Promise<Mode> { return v(1).then((x: i32) => v(x).then((y: i32) => \"b\")); }");
}

/// Rule 1 for each callback with a result context: the rejection callback
/// of `then(f, r)` and the callback of `catch`. Before this section each
/// accepted form gave the S013 that the control with a literal outside
/// `Mode` gives now.
#[test]
fn a_nested_then_in_a_rejection_callback_takes_the_context() {
    for body in [
        "async function h(): Promise<Mode> { return bad(1).then((x: i32): Mode => \"a\", (e: Error) => v(1).then((y: i32) => \"b\")); }",
        "async function h(q: Promise<Mode>): Promise<Mode> { return q.catch((e: Error) => v(1).then((y: i32) => \"b\")); }",
    ] {
        accepts(body);
    }
    for (body, message) in [
        (
            "async function h(): Promise<Mode> { return bad(1).then((x: i32): Mode => \"a\", (e: Error) => v(1).then((y: i32) => \"c\")); }",
            "a rejection callback of `.then(...)` must return `Mode`, the value of its fulfillment callback; it returns `string`",
        ),
        (
            "async function h(q: Promise<Mode>): Promise<Mode> { return q.catch((e: Error) => v(1).then((y: i32) => \"c\")); }",
            "a `catch` callback must return `Mode`, the value type of the handle; it returns `string`",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S013, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
}
