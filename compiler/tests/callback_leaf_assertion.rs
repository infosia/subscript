//! §313. Each case checks one short source, with no lowering or external process.
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const DECL: &str = "type CB = (x: i32) => i32;\nfunction one(x: i32): i32 { return x + 1; }\nfunction both(x: i32, y: i32): void {}\nasync function wf(x: f64): Promise<void> {}\nasync function work(x: i32): Promise<void> {}\nfunction run(f: (x: i32, y: i32) => void): void {}\nfunction each(f: (x: i32) => void): void {}\n";
fn errors(body: &str) -> Vec<Diagnostic> {
    let source =
        format!("{DECL}export function main(b: boolean, p: (x: i32) => void): void {{ {body} }}");
    check_program(&[SourceFile::new("test.ts", &source)]).expect_err(body)
}
fn assert_errors(body: &str, expected: &[(RuleCode, &str, &str)]) {
    let errors = errors(body);
    assert_eq!(errors.len(), expected.len(), "{errors:#?}");
    for (error, (code, message, token)) in errors.iter().zip(expected) {
        assert_eq!(error.code, *code, "{errors:#?}");
        assert_eq!(error.message, *message, "{errors:#?}");
        let line =
            format!("export function main(b: boolean, p: (x: i32) => void): void {{ {body} }}");
        assert_eq!(
            error.pos,
            subscript_compiler::Pos::new("test.ts", 8, (line.find(token).unwrap() + 1) as u32)
        );
    }
}
const ADAPTER: &str = "function value `p` needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it";
const HANDLE: &str = "`forEach` callback `work` returns `Promise<void>`, which the place `(i32) => void` drops, and no holder awaits the handle; a place with the result `Promise<void>` can await it";
const CAST: &str = "`as` converts between sized numerics, enum to integer, or narrows `object | null` to a class; cannot convert `(i32) => i32` to `(i32) => i32`";
const ARRAY_CAST: &str = "`as` converts between sized numerics, enum to integer, or narrows `object | null` to a class; cannot convert `((i32) => i32)[]` to `((i32) => i32)[]`";
/// Measured cost: 0.006 seconds, one test process with checker only (--test-threads=1).
#[test]
fn callback_parameter_leaf() {
    assert_errors(
        "[1].forEach(b ? p : both);",
        &[(RuleCode::S100, ADAPTER, "p :")],
    );
}
/// Measured cost: 0.005 seconds, one test process with checker only.
#[test]
fn callback_handle_leaves() {
    assert_errors(
        "[1].forEach(b ? wf : work);",
        &[
            (
                RuleCode::S100,
                "type mismatch: the callback expects `(i32) => void`, got `(f64) => Promise<void>`",
                "wf :",
            ),
            (RuleCode::S013, HANDLE, "work);"),
        ],
    );
}
/// Measured cost: 0.005 seconds, one test process with checker only.
#[test]
fn asserted_lambda() {
    assert_errors("((x) => x + 1) as CB;", &[(RuleCode::S100, CAST, "((x)")]);
}
/// Measured cost: 0.005 seconds, one test process with checker only.
#[test]
fn asserted_array_lambda() {
    assert_errors(
        "[one, (x) => x + 1] as CB[];",
        &[(RuleCode::S100, ARRAY_CAST, "[one")],
    );
}
/// Measured cost: 0.007 seconds, one test process with checker only.
#[test]
fn user_place_controls() {
    assert_errors("run(b ? p : both);", &[(RuleCode::S100, ADAPTER, "p :")]);
    assert_errors("each(b ? wf : work);", &[(RuleCode::S100, "type mismatch: the then branch expects `(i32) => void`, got `(f64) => Promise<void>`", "wf :"), (RuleCode::S013, "function value `work` returns `Promise<void>`, which the place `(i32) => void` drops, and no holder awaits the handle; a place with the result `Promise<void>` can await it", "work);")]);
    assert_errors("one as CB;", &[(RuleCode::S100, CAST, "one as")]);
    assert_errors("[one] as CB[];", &[(RuleCode::S100, ARRAY_CAST, "[one]")]);
}
