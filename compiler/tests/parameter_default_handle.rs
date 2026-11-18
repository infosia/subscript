//! A parameter default carries its handle into the body (compiler.md §329 rule 3).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
async function v(): Promise<i32> { return 1; }
async function u(): Promise<void> {}
"#;

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

/// The S013 diagnostics of a rejected body, as (line, column) pairs in
/// the body. The body starts on line 4 of the source.
fn s013_sites(body: &str) -> Vec<(u32, u32)> {
    let errors = check(body).expect_err(body);
    assert!(
        errors.iter().all(|error| error.code == RuleCode::S013),
        "{body}: {errors:?}"
    );
    errors
        .iter()
        .map(|error| (error.pos.line - 3, error.pos.col))
        .collect()
}

/// Rule 1: each form of the table of §257 is accepted, with a lambda
/// default and a constructor default.
#[test]
fn each_default_form_is_accepted() {
    for body in [
        "async function one(p: Promise<i32> = v()): Promise<i32> { return await p; }",
        "async function one(p: Promise<i32> = v(), k: i32 = 2): Promise<i32> { return (await p) + k; }",
        "async function one(p: Promise<void> = u()): Promise<void> { await p; }",
        "class C { async m(p: Promise<i32> = v()): Promise<i32> { return await p; } }",
        "const f = async (p: Promise<i32> = v()): Promise<i32> => await p;",
        "async function all(xs: Promise<i32>[] = [v(), v()]): Promise<i32> { let t = 0; for (const h of xs) { t += await h; } return t; }",
        "class K { p: Promise<i32>; constructor(q: Promise<i32> = v()) { this.p = q; } }",
    ] {
        if let Err(errors) = check(body) {
            panic!("{body}: {errors:?}");
        }
    }
}

/// A body that drops the default handle gives S013 at its call (§329 rule 3).
#[test]
fn an_undischarged_default_keeps_its_s013() {
    let body = "async function one(p: Promise<i32> = v()): Promise<i32> { return 1; }";
    assert_eq!(s013_sites(body), vec![(1, 38)]);
}

/// Control: a call of `v()` as a statement in the body keeps its S013.
#[test]
fn a_statement_call_keeps_its_s013() {
    let body = "async function one(p: Promise<i32> = v()): Promise<i32> { v(); return await p; }";
    assert_eq!(s013_sites(body), vec![(1, 59)]);
}
