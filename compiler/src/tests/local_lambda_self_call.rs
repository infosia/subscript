//! The checker facts for a local lambda self-call (compiler.md §285).
//! Cost: 45 checker calls, without code generation or external processes.

use super::*;

#[test]
fn a_local_self_call_has_no_capture_or_environment_ownership() {
    for (result, open, close) in [(": void", "", ""), ("", "", ""), (": void", "(", ")")] {
        let source = format!(
            "export function main(): void {{ const rec = {open}(n: i32){result} => {{ if (n > 0) rec(n - 1); }}{close}; rec(2); }}"
        );
        let module = check_one(&source).expect("self-call checks");
        let hir::Stmt::Let { init, .. } = &module.functions[0].body[0] else {
            panic!("lambda binding");
        };
        let hir::ExprKind::Lambda {
            id,
            captures,
            owns_environment,
            body,
            ..
        } = &init.kind
        else {
            panic!("lambda initializer");
        };
        assert!(captures.is_empty(), "{captures:?}");
        assert!(!owns_environment);
        fn calls(child: hir::HirChild<'_>, expected: hir::LambdaId) -> usize {
            let (count, children) = match child {
                hir::HirChild::Expr(expr) => (
                    usize::from(matches!(&expr.kind, hir::ExprKind::Call {
                        callee: hir::Callee::LambdaSelf { id, .. }, ..
                    } if *id == expected)),
                    expr.children(),
                ),
                hir::HirChild::Stmt(stmt) => (0, stmt.children()),
            };
            count
                + children
                    .into_iter()
                    .map(|child| calls(child, expected))
                    .sum::<usize>()
        }
        assert_eq!(
            body.iter()
                .map(|stmt| calls(hir::HirChild::Stmt(stmt), *id))
                .sum::<usize>(),
            1
        );
    }
}

#[test]
fn a_const_annotated_self_call_keeps_the_result_mismatch_diagnostic() {
    for (annotation, arrow, control, recursive, expected) in [
        (
            "(k: i32) => i32",
            "",
            "{ if (k > 0) k - 1; return \"bad\"; }",
            "{ if (k > 0) rec(k - 1); return \"bad\"; }",
            "type mismatch: the return value expects `i32`, got `string`",
        ),
        (
            "Result",
            "",
            "`${k}`",
            "`${rec(k)}`",
            "type mismatch: the lambda body expects `i32`, got `string`",
        ),
        (
            "(k: i32) => Promise<i32>",
            "async ",
            "{ if (k > 0) k - 1; return \"bad\"; }",
            "{ if (k > 0) await rec(k - 1); return \"bad\"; }",
            "type mismatch: the return value expects `i32`, got `string`",
        ),
    ] {
        for body in [control, recursive] {
            let source = format!(
                "type Result = (k: i32) => i32; export function main(): void {{ const rec: {annotation} = {arrow}(k) => {body}; }}"
            );
            let diagnostics = check_one(&source).expect_err("the result does not convert");
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code, RuleCode::S100);
            assert_eq!(diagnostics[0].message, expected);
        }
    }
}

fn rejects_self_call_with_accepted_control(
    annotation: &str,
    result: &str,
    control: &str,
    recursive: &str,
) {
    for declared in [annotation, "F"] {
        let source = |body| {
            format!(
                "type F = {annotation}; export function main(): void {{ const rec: {declared} = (k){result} => {body}; }}"
            )
        };
        check_one(&source(control)).expect("the control without a self-call checks");
        let diagnostics = check_one(&source(recursive)).expect_err("the self-call stays rejected");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(
            diagnostics[0].message,
            "`rec` is read before its declaration in this block"
        );
    }
}

#[test]
fn a_self_call_rejects_a_const_annotation_with_more_parameters() {
    for result in ["", ": i32"] {
        rejects_self_call_with_accepted_control(
            "(k: i32, d: i32) => i32",
            result,
            "k <= 1 ? 1 : k * (k - 1)",
            "k <= 1 ? 1 : k * rec(k - 1)",
        );
    }
}

#[test]
fn a_self_call_rejects_a_nullable_const_annotation() {
    for result in ["", ": i32"] {
        rejects_self_call_with_accepted_control(
            "((k: i32) => i32) | null",
            result,
            "k <= 1 ? 1 : k * (k - 1)",
            "k <= 1 ? 1 : k * rec(k - 1)",
        );
    }
}

#[test]
fn a_self_call_rejects_a_nullable_type_alias() {
    for result in ["", ": i32"] {
        let source = |body| {
            format!(
                "type G = ((k: i32) => i32) | null; export function main(): void {{ const rec: G = (k){result} => {body}; }}"
            )
        };
        check_one(&source("k <= 1 ? 1 : k * (k - 1)"))
            .expect("the control without a self-call checks");
        let diagnostics = check_one(&source("k <= 1 ? 1 : k * rec(k - 1)"))
            .expect_err("the nullable alias rejects the self-call");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(
            diagnostics[0].message,
            "`rec` is read before its declaration in this block"
        );
    }
}

#[test]
fn a_void_block_self_call_rejects_a_const_annotation_with_more_parameters() {
    for result in ["", ": void"] {
        rejects_self_call_with_accepted_control(
            "(k: i32, d: i32) => void",
            result,
            "{ if (k > 0) k - 1; }",
            "{ if (k > 0) rec(k - 1); }",
        );
    }
}

#[test]
fn a_void_block_self_call_rejects_a_nullable_const_annotation() {
    for result in ["", ": void"] {
        rejects_self_call_with_accepted_control(
            "((k: i32) => void) | null",
            result,
            "{ if (k > 0) k - 1; }",
            "{ if (k > 0) rec(k - 1); }",
        );
    }
}
