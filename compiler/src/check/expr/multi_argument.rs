//! Calls with a fixed argument list of any length: `push`, `Math.max`, and
//! `Math.min` (compiler.md §201).

use swc_ecma_ast as ast;

use crate::check::rejection::RejectionSite;
use crate::check::{Checker, FnCtx, ParamSig, SyntheticUse};
use crate::diag::Pos;
use crate::hir::{self, Callee, ExprKind, MathFn};
use crate::types::Type;

/// The method name of an append.
const PUSH: &str = "push";

impl<'p> Checker<'p> {
    /// Checks `r.push(a1, …, ak)` on an `Array` receiver (§201.1 rule 1).
    /// With no argument the result is the length. With one argument the
    /// result is one append. With more, every argument is evaluated before
    /// the first append, in one `Seq` (§201.1 rule 4a).
    pub(super) fn check_array_push(
        &mut self,
        recv: hir::Expr,
        elem: Type,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let Some(mut args) = self.check_push_arguments(&elem, &c.args, fx) else {
            return self.err_expr(pos);
        };
        match args.len() {
            0 => hir::Expr {
                pending_work: None,
                kind: ExprKind::Length(Box::new(recv)),
                ty: Type::I32,
                pos,
            },
            1 => push_call(recv, args.remove(0), pos),
            _ => self.push_sequence(recv, args, fx, pos),
        }
    }

    /// Checks each argument against the element type. A spread argument
    /// keeps its existing rejection (§201.1 rule 5).
    fn check_push_arguments(
        &mut self,
        elem: &Type,
        args: &[ast::ExprOrSpread],
        fx: &mut FnCtx,
    ) -> Option<Vec<hir::Expr>> {
        let mut checked = Vec::with_capacity(args.len());
        let mut spread = false;
        for argument in args {
            if let Some(span) = argument.spread {
                let spread_pos = self.pos(span);
                self.reject_subset(
                    RejectionSite::CallSpread,
                    "spread arguments require variadic parameters, which the language does not have",
                    spread_pos,
                );
                spread = true;
                continue;
            }
            let value = self.check_expr(&argument.expr, Some(elem), fx);
            self.require_expr_assignable(&value, elem, fx, "the argument");
            if self.transfers_async_origins(elem) {
                let origins = self.expr_async_origins(&value, fx);
                fx.handle_async_origins(&origins);
            }
            checked.push(value);
        }
        (!spread).then_some(checked)
    }

    /// Builds the `Seq` of §201.1 rule 4a. Each argument that is not free
    /// of order is stored in a synthetic local at its evaluation. If one
    /// is, the receiver is read into a synthetic local before the first
    /// argument, whatever its kind (§201.1 rule 4). Otherwise only a
    /// non-place receiver is stored, so that it evaluates once.
    fn push_sequence(
        &mut self,
        recv: hir::Expr,
        args: Vec<hir::Expr>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let mut effects = Vec::new();
        let ordered = !args.iter().all(is_order_free);
        let role = if super::is_place_expr(&recv) {
            ordered.then_some(SyntheticUse::PushArgument)
        } else {
            Some(SyntheticUse::PushReceiver)
        };
        let recv = match role {
            Some(role) => {
                let (store, read) = self.bind_synthetic_operand(recv, "receiver", role, fx);
                effects.push(store);
                read
            }
            None => recv,
        };
        let mut values = Vec::with_capacity(args.len());
        for argument in args {
            if is_order_free(&argument) {
                values.push(argument);
            } else {
                let (store, read) = self.bind_synthetic_operand(
                    argument,
                    "argument",
                    SyntheticUse::PushArgument,
                    fx,
                );
                effects.push(store);
                values.push(read);
            }
        }
        let Some(last) = values.pop() else {
            return self.err_expr(pos);
        };
        for value in values {
            effects.push(push_call(recv.clone(), value, pos.clone()));
        }
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Seq {
                effects,
                value: Box::new(push_call(recv, last, pos.clone())),
            },
            ty: Type::I32,
            pos,
        }
    }

    /// Checks `Math.max` or `Math.min` with any argument count other than
    /// two (§201.1 rule 2), or with a spread argument, which gives the
    /// call-spread text (rule 5). Returns `None` for every other call,
    /// which the two-argument path checks.
    pub(super) fn check_math_fold(
        &mut self,
        f: MathFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<hir::Expr> {
        let spread = c.args.iter().any(|argument| argument.spread.is_some());
        if !matches!(f, MathFn::Max | MathFn::Min) || (c.args.len() == 2 && !spread) {
            return None;
        }
        let params = vec![ParamSig::positional(Type::F64); c.args.len()];
        let args = self.check_args(
            RejectionSite::MathMethodArgumentCount,
            &params,
            &c.args,
            fx,
            pos,
            &format!("Math.{}", f.name()),
        );
        if spread {
            return Some(self.err_expr(pos.clone()));
        }
        let mut args = args.into_iter();
        let Some(first) = args.next() else {
            let empty = if f == MathFn::Max {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
            return Some(hir::Expr {
                pending_work: None,
                kind: ExprKind::Float(empty),
                ty: Type::F64,
                pos: pos.clone(),
            });
        };
        Some(args.fold(first, |left, right| hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Math(f),
                args: vec![left, right],
            },
            ty: Type::F64,
            pos: pos.clone(),
        }))
    }
}

/// One append `recv.push(value)`.
fn push_call(recv: hir::Expr, value: hir::Expr, pos: Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind: ExprKind::Call {
            callee: Callee::Method {
                recv: Box::new(recv),
                name: hir::Symbol::from_full_text(PUSH),
            },
            args: vec![value],
        },
        ty: Type::I32,
        pos,
    }
}

/// A literal, a function reference, or a lambda has no effect, and a
/// lambda captures only bindings that do not change (C5), so its
/// evaluation can move after the appends before it. A lambda keeps the
/// capture facts of a direct argument this way.
fn is_order_free(expression: &hir::Expr) -> bool {
    matches!(
        expression.kind,
        ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Null
            | ExprKind::EnumMember { .. }
            | ExprKind::FuncRef(_)
            | ExprKind::Lambda { .. }
    )
}

#[cfg(test)]
#[path = "multi_argument_tests.rs"]
mod tests;
