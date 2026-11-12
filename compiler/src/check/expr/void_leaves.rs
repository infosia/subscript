//! The `void` leaves of a conditional, `&&`, and `||` in a statement place,
//! and of a conditional body of a lambda with no result (compiler.md §258).

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
    /// operand of `&&` and `||` is a statement place (§258 rule 2).
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
        let checked = self.check_expr_keeping_void_as(inner, ctx, kind, fx);
        self.void_leaf_place = None;
        checked
    }

    /// The type of a conditional with the leaves `then` and `els` in
    /// `place`, when a leaf is `void`. A statement place drops each value
    /// (§258 rule 2). Else each leaf must be `void`, and a `void` leaf
    /// beside a value leaf is a `void` value (§258 rule 3).
    pub(super) fn void_leaf_join(
        &mut self,
        then: &hir::Expr,
        els: &hir::Expr,
        ctx: Option<&Type>,
        place: VoidLeafPlace,
    ) -> Option<Type> {
        let then_ty = self.apparent_type(&then.ty);
        let els_ty = self.apparent_type(&els.ty);
        let void_leaf = match (&then_ty, &els_ty) {
            (Type::Void, Type::Void) => return Some(Type::Void),
            (Type::Void, _) => then,
            (_, Type::Void) => els,
            _ => return None,
        };
        if place == VoidLeafPlace::Statement {
            return Some(Type::Void);
        }
        if then_ty != Type::Error && els_ty != Type::Error {
            self.reject_void_value(ctx, void_leaf.pos.clone());
        }
        Some(Type::Error)
    }

    /// The statements of a checked expression in a statement place. A `void`
    /// conditional is an `if` with one branch per leaf, `a && b` is
    /// `if (a) b;`, and `a || b` is `if (a) {} else b;`, so the order of
    /// evaluation and the short circuit stay (§258 rules 2 and 5).
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
