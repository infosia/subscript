//! Discarded leaves in a statement place (compiler.md §300), and `void`
//! leaves of a conditional body of a lambda with no result (§258 rule 3).

use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx};
use crate::hir::{self, BinOp, ExprKind};
use crate::types::Type;

use super::entry::ContextKind;
use super::unparen_expr;

/// The place of the leaves of the next conditional, `&&`, or `||`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VoidLeafPlace {
    /// A statement place. Each branch of a conditional and the right
    /// operand of `&&` and `||` is a statement place (§300 rule 1).
    Statement,
    /// The expression body of a lambda with no result annotation and no
    /// context. A conditional whose leaves are each `void` is `void`
    /// (§258 rule 3).
    VoidResult,
}

/// True when `e` takes its leaves from `place`: a conditional in each
/// place, and `&&` or `||` in a statement place only.
fn takes_leaf_place(e: &ast::Expr, place: VoidLeafPlace) -> bool {
    match e {
        ast::Expr::Cond(_) => true,
        ast::Expr::Bin(binary) => {
            place == VoidLeafPlace::Statement
                && matches!(
                    binary.op,
                    ast::BinaryOp::LogicalAnd | ast::BinaryOp::LogicalOr
                )
        }
        _ => false,
    }
}

impl<'p> Checker<'p> {
    /// Sets `place` for `e` when `e` is a conditional, or `&&` or `||` in
    /// a statement place. A parenthesized expression keeps the place
    /// (§258 rule 1). Returns `e` without its parentheses.
    pub(super) fn enter_void_leaf_place<'e>(
        &mut self,
        e: &'e ast::Expr,
        place: VoidLeafPlace,
    ) -> &'e ast::Expr {
        let inner = unparen_expr(e);
        self.void_leaf_place = takes_leaf_place(inner, place).then_some(place);
        inner
    }

    /// Checks one leaf of a conditional, `&&`, or `||` in `place`. A
    /// `void` leaf keeps its type.
    pub(super) fn check_void_leaf(
        &mut self,
        e: &ast::Expr,
        ctx: Option<&Type>,
        kind: ContextKind,
        place: VoidLeafPlace,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let inner = self.enter_void_leaf_place(e, place);
        // compiler.md §339 rule 7: an optional call at a statement leaf is a statement.
        if place == VoidLeafPlace::Statement {
            if let ast::Expr::OptChain(chain) = inner {
                return self.check_optional_chain_leaf(chain, fx);
            }
        }
        let checked = self.check_expr_keeping_void_as(inner, ctx, kind, fx);
        self.void_leaf_place = None;
        checked
    }

    /// A statement place drops each value without a common type (§300
    /// rule 1). Else each leaf must be `void`, and a `void` leaf
    /// beside a value leaf is a `void` value (§258 rule 3).
    pub(super) fn void_leaf_join(
        &mut self,
        then: &hir::Expr,
        els: &hir::Expr,
        ctx: Option<&Type>,
        place: VoidLeafPlace,
    ) -> Option<Type> {
        if place == VoidLeafPlace::Statement {
            return Some(Type::Void);
        }
        let then_ty = self.apparent_type(&then.ty);
        let els_ty = self.apparent_type(&els.ty);
        let void_leaf = match (&then_ty, &els_ty) {
            (Type::Void, Type::Void) => return Some(Type::Void),
            (Type::Void, _) => then,
            (_, Type::Void) => els,
            _ => return None,
        };
        if then_ty != Type::Error && els_ty != Type::Error {
            self.reject_void_value(ctx, void_leaf.pos.clone());
        }
        Some(Type::Error)
    }

    /// The statements of a checked expression in a statement place. A `void`
    /// conditional is an `if` with one branch per leaf, `a && b` is
    /// `if (a) b;`, and `a || b` is `if (a) {} else b;`, so the order of
    /// evaluation and the short circuit stay (§300 rules 1 and 5). Each
    /// leaf keeps its type for the ordinary expression-statement release.
    pub(super) fn void_leaf_statements(&self, expr: hir::Expr) -> Vec<hir::Stmt> {
        if self.apparent_type(&expr.ty) != Type::Void || expr.pending_work.is_some() {
            return vec![hir::Stmt::Expr(expr)];
        }
        let pos = expr.pos.clone();
        match expr.kind {
            ExprKind::Cond { cond, then, els } => vec![hir::Stmt::If {
                cond: *cond,
                then: self.void_leaf_statements(*then),
                els: Some(self.void_leaf_statements(*els)),
                pos,
            }],
            ExprKind::Binary {
                op: BinOp::And,
                left,
                right,
            } => vec![hir::Stmt::If {
                cond: *left,
                then: self.void_leaf_statements(*right),
                els: None,
                pos,
            }],
            ExprKind::Binary {
                op: BinOp::Or,
                left,
                right,
            } => vec![hir::Stmt::If {
                cond: *left,
                then: Vec::new(),
                els: Some(self.void_leaf_statements(*right)),
                pos,
            }],
            kind => vec![hir::Stmt::Expr(hir::Expr {
                pending_work: None,
                kind,
                ty: Type::Void,
                pos,
            })],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_program, RuleCode, SourceFile};

    // compiler.md §300 rule 1. Cost: one short checker run, no codegen or
    // external process; measured at 0.01 s in the debug test harness.
    #[test]
    fn statement_branches_do_not_share_literal_context() {
        let source = r#"
function small(): u8 { return 1; }
type Tag = "a" | "b";
function tag(): Tag { return "a"; }
export function main(): void {
  const b = true;
  b ? small() : 300;
  b ? 300 : small();
  b ? tag() : "c";
  b ? "c" : tag();
  const cb: (b: boolean) => void = (b: boolean) => b ? small() : 300;
  cb(b);
}
"#;
        check_program(&[SourceFile::new("test.ts", source)])
            .expect("each discarded branch checks without its peer's literal context");
        let control = source.replace("  b ? small() : 300;", "  const x = b ? small() : 300;");
        let errors = check_program(&[SourceFile::new("test.ts", control)])
            .expect_err("a value conditional keeps its peer's literal context");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S008);
        assert_eq!(
            errors[0].message,
            "integer literal 300 out of range for `u8`"
        );
    }

    // compiler.md §300 acceptance item 3. Cost: three short checker runs;
    // measured at 0.02 s in the debug test harness. No codegen or external
    // process runs in the test.
    #[test]
    fn statement_value_leaves_keep_truth_and_async_checks() {
        let check = |body: &str| {
            let source = format!(
                "function k(): i32 {{ return 1; }}\n\
                 async function v(): Promise<void> {{}}\n\
                 export function main(): void {{\n  const b = true;\n  {body}\n}}\n"
            );
            check_program(&[SourceFile::new("test.ts", source)])
        };
        check("b && k();").expect("a statement place drops an i32 leaf");
        for (body, code, message, column) in [
            (
                "5 && k();",
                RuleCode::S100,
                "logical operators require booleans, got `i32`",
                3,
            ),
            (
                "b && v();",
                RuleCode::S013,
                "an async handle is dropped without any await of its completion",
                8,
            ),
        ] {
            let errors = check(body).expect_err(body);
            assert_eq!(errors.len(), 1, "{body}: {errors:?}");
            let error = &errors[0];
            assert_eq!((error.code, error.message.as_str()), (code, message));
            assert_eq!((error.pos.line, error.pos.col), (5, column));
        }
    }
}
