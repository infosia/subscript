//! The checker facts for a local lambda self-call (compiler.md §284).
//! Cost: three checker calls, without code generation or external processes.

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
