//! Optional lambda context and optional handle returns (compiler.md §318).

use subscript_compiler::{check_program, hir, RuleCode, SourceFile, Type};

fn check(source: &str) -> Result<hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::new("optional.ts", source)])
}

// Cost: six short checker runs; measured below 0.02 s, with no external process.
#[test]
fn both_absence_spellings_give_the_function_context() {
    for absence in ["null", "undefined"] {
        for lambda in ["(x) => 7 / 2", "(x: i32) => 7 / 2", "((x) => 7 / 2)"] {
            let module = check(&format!(
                "type CB = (x: i32) => f64; function r(): CB | {absence} {{ return {lambda}; }}"
            ))
            .expect("optional context");
            let function = module.functions.iter().find(|f| f.name == "r").expect("r");
            let hir::Stmt::Return {
                value: Some(value), ..
            } = &function.body[0]
            else {
                panic!("{:?}", function.body);
            };
            assert_eq!(value.ty, Type::func(vec![Type::I32], Type::F64));
            let hir::ExprKind::Lambda {
                params, ret, body, ..
            } = &value.kind
            else {
                panic!("{value:?}");
            };
            assert_eq!(params[0].ty, Type::I32);
            assert_eq!(*ret, Type::F64);
            assert!(
                matches!(&body[0], hir::Stmt::Return { value: Some(value), .. } if value.ty == Type::F64)
            );
        }
    }
}

// Cost: four short checker runs; measured below 0.02 s, with no external process.
#[test]
fn the_function_context_checks_the_result_and_the_annotated_parameter() {
    let mut failures = Vec::new();
    for (absence, body_col, lambda_col) in [("null", 68, 61), ("undefined", 73, 66)] {
        for (lambda, message, col) in [
            (
                "(x) => \"wrong\"",
                "type mismatch: the lambda body expects `f64`, got `string`".to_string(),
                body_col,
            ),
            (
                "(x: string) => 1",
                format!("type mismatch: the return value expects `((i32) => f64) | {absence}`, got `(string) => f64`"),
                lambda_col,
            ),
        ] {
            let diagnostics = check(&format!(
                "type CB = (x: i32) => f64; function r(): CB | {absence} {{ return {lambda}; }}"
            ))
            .expect_err("incompatible lambda");
            if diagnostics.len() != 1
                || diagnostics[0].code != RuleCode::S100
                || diagnostics[0].message != message
                || (diagnostics[0].pos.line, diagnostics[0].pos.col) != (1, col)
            {
                failures.push(format!("{absence} {lambda}: {diagnostics:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Cost: five short checker runs; measured below 0.02 s, with no external process.
#[test]
fn optional_void_handles_adopt_in_async_functions_and_lambdas() {
    for declaration in [
        "async function g(s: S | null): Promise<void> { return s?.am(); }",
        "async function g(s: S | null): Promise<void> { return (s?.am()); }",
        "async function g(s: S | null): Promise<void> { return true ? s?.am() : undefined; }",
        "function g(): void { const f = async (s: S | null): Promise<void> => s?.am(); }",
        "function g(): void { const f: (s: S | null) => Promise<void> = async (s) => { return s?.am(); }; }",
    ] {
        check(&format!("class S {{ async am(): Promise<void> {{}} }} {declaration}"))
            .expect(declaration);
    }
}

// Cost: four short checker runs; measured below 0.02 s, with no external process.
#[test]
fn other_optional_results_keep_the_void_return_diagnostic() {
    for (result, asynchronous, returned) in [
        ("i32", "", "return 1;"),
        ("Promise<i32>", "async ", "return 1;"),
    ] {
        for function in [
            "function g(s: S | null): void",
            "async function g(s: S | null): Promise<void>",
        ] {
            let diagnostics = check(&format!(
                "class S {{ {asynchronous}am(): {result} {{ {returned} }} }} {function} {{ return s?.am(); }}"
            ))
            .expect_err("value result");
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.message == "a `void` function cannot return a value"),
                "{diagnostics:?}"
            );
        }
    }
}

// Cost: two short checker runs, with no external process.
#[test]
fn optional_context_reject_entries_check_the_message_and_column() {
    let mut failures = Vec::new();
    for (source, message, col) in [
        (
            include_str!("../../corpus/reject/r536-optional-lambda-result.ts"),
            "type mismatch: the lambda body expects `i32`, got `string`",
            46,
        ),
        (
            include_str!("../../corpus/reject/r537-optional-lambda-parameter.ts"),
            "type mismatch: the return value expects `((i32) => f64) | undefined`, got `(string) => f64`",
            39,
        ),
    ] {
        let diagnostics = check(source).expect_err("incompatible lambda");
        if diagnostics.len() != 1
            || diagnostics[0].code != RuleCode::S100
            || diagnostics[0].message != message
            || (diagnostics[0].pos.line, diagnostics[0].pos.col) != (8, col)
        {
            failures.push(format!("{source}: {diagnostics:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
