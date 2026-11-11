//! A `void` result place names the handle that it drops (compiler.md
//! §247).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
type CB = (x: i32) => void;
function each(xs: i32[], f: CB): void { for (const x of xs) { f(x); } }
async function work(x: i32): Promise<void> { print(`${x}`); }
async function val(x: i32): Promise<i32> { return x; }
function jobs(x: i32): Promise<i32>[] { return []; }
function* gen(x: i32): Generator<i32> { yield x; }
"#;

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

/// The diagnostics of a rejected body.
fn rejects(body: &str) -> Vec<Diagnostic> {
    check(body).expect_err(body)
}

/// The text of rule 2 for `subject` with the result `handle` in the place
/// `(i32) => void`.
fn rule2(subject: &str, handle: &str) -> String {
    format!(
        "{subject} returns `{handle}`, which the place `(i32) => void` drops, and no holder awaits the handle; a place with the result `{handle}` can await it"
    )
}

fn assert_rule2(errors: &[Diagnostic], message: &str, body: &str) {
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    let error = &errors[0];
    assert_eq!(error.code, RuleCode::S013, "{body}: {error:?}");
    assert_eq!(error.message, message, "{body}");
    assert_eq!(
        error.divergence,
        Some(Divergence::DroppedAsyncHandle),
        "{body}"
    );
}

/// Rules 1 and 2: each form of the table of §247 gets S013 with the
/// value, the handle type, and the place type. A lambda that calls the
/// async function keeps the S013 of the drop check.
#[test]
fn each_form_names_the_handle_it_drops() {
    for (body, message) in [
        (
            "function f(): void { each([1], work); }",
            rule2("function value `work`", "Promise<void>"),
        ),
        (
            "function f(): void { const g: CB = work; g(1); }",
            rule2("function value `work`", "Promise<void>"),
        ),
        (
            "function f(): void { each([1], val); }",
            rule2("function value `val`", "Promise<i32>"),
        ),
        (
            "function f(): void { each([1], async (x: i32): Promise<void> => {}); }",
            rule2("function value", "Promise<void>"),
        ),
        (
            "function f(): void { const w = work; each([1], w); }",
            rule2("function value `w`", "Promise<void>"),
        ),
        (
            "function f(): void { [1].forEach(work); }",
            rule2("`forEach` callback `work`", "Promise<void>"),
        ),
        (
            "function f(): void { [1].forEach(async (x: i32): Promise<void> => {}); }",
            rule2("`forEach` callback", "Promise<void>"),
        ),
        (
            "function f(): void { [1].forEach(async (): Promise<void> => {}); }",
            "`forEach` callback returns `Promise<void>`, which the place `(i32) => void` drops, and no holder awaits the handle; a place with the result `Promise<void>` can await it".to_string(),
        ),
        (
            "function f(): void { each([1], jobs); }",
            rule2("function value `jobs`", "Promise<i32>[]"),
        ),
        (
            "function f(cb: (x: i32) => Promise<void>): void { each([1], cb); }",
            rule2("function value `cb`", "Promise<void>"),
        ),
    ] {
        assert_rule2(&rejects(body), &message, body);
    }
    let body = "function f(): void { each([1], (x: i32) => work(x)); }";
    let errors = rejects(body);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S013);
    assert_eq!(
        errors[0].message,
        "an async handle is dropped without any await of its completion"
    );
}

/// Rule 3: the leaf of a conditional that holds the handle gets the
/// diagnostic of rule 1, in a user place and in a built-in callback. The
/// leaf of a callback names the method, as the callback itself does.
#[test]
fn a_conditional_leaf_names_the_handle() {
    let body = "function f(b: boolean): void { each([1], b ? work : (x: i32): void => {}); }";
    assert_rule2(
        &rejects(body),
        &rule2("function value `work`", "Promise<void>"),
        body,
    );
    let body = "function f(b: boolean): void { [1].forEach(b ? work : (x: i32): void => {}); }";
    assert_rule2(
        &rejects(body),
        &rule2("`forEach` callback `work`", "Promise<void>"),
        body,
    );
}

/// Rule 1: a result that contains the error type has its diagnostic
/// already, and gets no S013. The control: the same place with a handle
/// result gets S013 above.
#[test]
fn a_poisoned_result_gets_no_second_diagnostic() {
    let body = "async function* ag(x: i32): AsyncGenerator<i32> { yield x; }\nfunction f(): void { each([1], ag); }";
    let errors = rejects(body);
    assert!(
        errors.iter().all(|error| error.code != RuleCode::S013),
        "{errors:?}"
    );
}

/// Rule 4: a value whose parameters do not fit keeps its present
/// diagnostic, and a counted result in a place that does not drop it
/// keeps §171. The firing control: the same places with a fitting value
/// get S013 above.
#[test]
fn the_controls_keep_their_diagnostic() {
    for (body, code, message) in [
        (
            "async function ws(s: string): Promise<void> {}\nfunction f(): void { each([1], ws); }",
            RuleCode::S100,
            "type mismatch: the argument expects `(i32) => void`, got `(string) => Promise<void>`",
        ),
        (
            "function f(): void { each([1], async (s: string): Promise<void> => {}); }",
            RuleCode::S100,
            "type mismatch: the argument expects `(i32) => void`, got `(string) => Promise<void>`",
        ),
        (
            "function f(): void { [1].forEach(async (s: string): Promise<void> => {}); }",
            RuleCode::S014,
            "`forEach` cannot carry a counted callback result (§171)",
        ),
        (
            "function mkg(x: i32): Generator<i32> { return gen(x); }\nfunction f(): void { const a = [1].map(mkg); }",
            RuleCode::S014,
            "`map` cannot carry a counted callback result (§171); a generator is a counted type (§176). Use `for (const value of values) { ... }`.",
        ),
        (
            "function f(): void { const a = [1].map(work); }",
            RuleCode::S014,
            "`map` cannot carry a counted callback result (§171)",
        ),
    ] {
        let errors = rejects(body);
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, code, "{body}: {errors:?}");
        assert_eq!(errors[0].message, message, "{body}");
    }
}
