//! A `FixedArray` length is at least 1 (compiler.md §254 rule 1).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const MESSAGE: &str = "a `FixedArray` length is at least 1; C has no array with the length zero";

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

/// Rule 1: the length `0` is rejected at a type annotation, a type
/// argument, and a result type, with the divergence block: `tsc` accepts
/// the type.
#[test]
fn the_length_zero_is_rejected() {
    for body in [
        "function f(): void { const a: FixedArray<i32, 0> = []; }",
        "function f(): void { const e: FixedArray<i32, 0>[] = []; }",
        "function f(): void { const m = new Map<string, FixedArray<i32, 0>>(); }",
        "function mk(): FixedArray<i32, 0> { return []; }",
    ] {
        let errors = check(body).expect_err(body);
        let error = errors
            .iter()
            .find(|error| error.message == MESSAGE)
            .unwrap_or_else(|| panic!("{body}: {errors:?}"));
        assert_eq!(error.code, RuleCode::S100, "{body}");
        assert_eq!(
            error.divergence,
            Some(Divergence::FixedArrayLengthZero),
            "{body}"
        );
    }
}

/// Rule 3: the length `1` stays accepted. The firing control: the forms
/// above.
#[test]
fn the_length_one_is_accepted() {
    for body in [
        "function f(): void { const a: FixedArray<i32, 1> = [7]; }",
        "function mk(): FixedArray<i32, 1> { return [1]; }",
    ] {
        check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
}
