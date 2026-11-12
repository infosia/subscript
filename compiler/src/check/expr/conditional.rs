//! Checks the conditional expression (compiler.md §146, §195.1, §242
//! rule 1a, §248 rule 2, §249 rule 1, §258).

use crate::check::narrowing_fact::FactSet;
use crate::check::rejection::RejectionSite;
use std::collections::HashSet;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx};
use crate::diag::Pos;
use crate::hir::{self, ExprKind};
use crate::types::Type;

use super::literalish;

impl<'p> Checker<'p> {
    // C4 and Q32: a literal can adopt a numeric or string-alias context.
    pub(super) fn literal_context(&self, ty: &Type) -> Option<Type> {
        (self.apparent_type(ty).is_numeric()
            || matches!(self.apparent_type(ty), Type::StringAlias(_)))
        .then(|| ty.clone())
    }

    /// Whether one operand has the `null` spelling and the other has the
    /// `undefined` spelling (compiler.md §195.1 rule 3). An operand has the
    /// `null` spelling if it is the `null` literal or a `T | null` value. A
    /// generic union has the spelling of each member (§195.11 rule 5).
    pub(super) fn joins_null_and_undefined(&self, left: &Type, right: &Type) -> bool {
        let spelled = |ty: &Type, absence: crate::types::Absence| {
            let types = match self.apparent_type(ty) {
                Type::GenericUnion(types) => types.into_vec(),
                other => vec![other],
            };
            types.iter().any(|member| {
                member.absence() == Some(absence)
                    || (*member == Type::Null && absence == crate::types::Absence::Null)
            })
        };
        let (null, undefined) = (
            crate::types::Absence::Null,
            crate::types::Absence::Undefined,
        );
        (spelled(left, null) && spelled(right, undefined))
            || (spelled(right, null) && spelled(left, undefined))
    }

    /// Reports the S011 of a join that holds both absence spellings
    /// (compiler.md §195.1 rule 3).
    pub(super) fn reject_both_spellings(&mut self, pos: Pos) {
        self.reject_subset(
            RejectionSite::UndefinedNullUnion,
            "`T | null | undefined` has no representation; a nullable type has one absence spelling, `null` or `undefined`",
            pos,
        );
    }

    // §146: equal types and legal nullable pairs have a symmetric join.
    // §195.1 rule 2: the `null` literal joins the `null` spelling only.
    fn conditional_join(&self, left: &Type, right: &Type) -> Option<Type> {
        if matches!(left, Type::Error) || matches!(right, Type::Error) {
            return Some(Type::Error);
        }
        if left == right {
            return Some(left.clone());
        }
        for (value, other) in [(left, right), (right, left)] {
            if let Type::Nullable(inner, spelling) = value {
                if (other == inner.as_ref()
                    || (matches!(other, Type::Null) && *spelling == crate::types::Absence::Null))
                    && self.allows_nullable(inner)
                {
                    return Some(value.clone());
                }
            }
            if matches!(other, Type::Null)
                && value.absence().is_none()
                && self.allows_nullable(value)
            {
                return Some(Type::nullable(value.clone()));
            }
        }
        None
    }

    pub(super) fn check_cond(
        &mut self,
        c: &ast::CondExpr,
        (ctx, kind): (Option<&Type>, super::entry::ContextKind),
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let leaves = self.callback_leaf_place.take();
        let void_leaves = self.void_leaf_place.take();
        let cond = self.check_truth_expr(&c.test, fx);
        if !self.instance_restriction(
            crate::check::opaque::InstanceRestriction::BooleanContext,
            &cond.ty,
        ) && !matches!(self.apparent_type(&cond.ty), Type::Bool | Type::Error)
        {
            let name = self.type_name(&cond.ty);
            self.reject_subset(
                if matches!(
                    self.apparent_type(&cond.ty),
                    Type::AsyncHandle(_) | Type::Func(_)
                ) {
                    RejectionSite::ConditionalAlwaysTruthyCondition
                } else {
                    RejectionSite::ConditionalNonBooleanCondition
                },
                format!("condition must be boolean, got `{}`", name),
                cond.pos.clone(),
            );
        }
        let mut note_paths = fx.narrowing_note_paths();
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);
        let base = fx.narrowed.clone();
        let initial_notes = fx.ended_shared_narrowing.clone();

        // Check the nonliteral arm first so either literal arm can take its context.
        let reverse = ctx.is_none() && literalish(&c.cons) && !literalish(&c.alt);
        let (first_ast, second_ast, first_extra, second_extra) = if reverse {
            (&c.alt, &c.cons, else_extra, then_extra)
        } else {
            (&c.cons, &c.alt, then_extra, else_extra)
        };
        fx.narrowed = base
            .iter()
            .filter(|key| !second_extra.contains(key))
            .cloned()
            .chain(first_extra.clone())
            .collect();
        let first_ctx = self.leaf_context(first_ast, leaves.as_ref(), ctx);
        // compiler.md §249 rule 1: a branch takes the kind of the context.
        let branch_kind = |leaf_place: bool| match leaf_place {
            true => super::entry::ContextKind::Contextual,
            false => kind,
        };
        let first = match void_leaves {
            Some(place) => self.check_void_leaf(
                first_ast,
                first_ctx.as_ref(),
                branch_kind(leaves.is_some()),
                place,
                fx,
            ),
            None => self.check_expr_as(
                first_ast,
                first_ctx.as_ref(),
                branch_kind(leaves.is_some()),
                fx,
            ),
        };
        let first_facts = fx.narrowed.clone();
        let first_notes = fx.ended_shared_narrowing.clone();

        let literal_ctx = if ctx.is_none() && literalish(second_ast) {
            self.literal_context(&first.ty)
        } else {
            None
        };
        fx.ended_shared_narrowing = initial_notes;
        fx.narrowed = base
            .iter()
            .filter(|key| !first_extra.contains(key))
            .cloned()
            .chain(second_extra)
            .collect();
        let second_ctx =
            self.leaf_context(second_ast, leaves.as_ref(), ctx.or(literal_ctx.as_ref()));
        let second_kind = match ctx {
            None if leaves.is_none() => super::entry::ContextKind::Hint,
            _ => branch_kind(leaves.is_some()),
        };
        let second = match void_leaves {
            Some(place) => {
                self.check_void_leaf(second_ast, second_ctx.as_ref(), second_kind, place, fx)
            }
            None => self.check_expr_as(second_ast, second_ctx.as_ref(), second_kind, fx),
        };
        let first_possible: HashSet<_> = first_facts.union(&first_notes).cloned().collect();
        let second_possible: HashSet<_> = fx
            .narrowed
            .union(&fx.ended_shared_narrowing)
            .cloned()
            .collect();
        note_paths.extend_facts(first_possible.intersection(&second_possible).cloned());
        let joined = first_facts.intersection(&fx.narrowed).cloned().collect();
        let (mut then, mut els) = if reverse {
            (second, first)
        } else {
            (first, second)
        };
        fx.narrowed = joined;
        fx.ended_shared_narrowing.extend_facts(first_notes);
        fx.finish_narrowing_join(&note_paths);

        // compiler.md §242 rule 1a and §244 rule 2: the outer conditional
        // places each leaf after all of them are checked.
        let nested = leaves.as_ref().is_some_and(|leaves| leaves.nested);
        if let Some(leaves) = leaves.as_ref().filter(|leaves| !leaves.nested) {
            self.place_callback_leaves(&mut then, &mut els, leaves, fx);
        }
        // compiler.md §258 rules 2 and 3: a `void` leaf in a statement
        // place, or beside another `void` leaf in a lambda body.
        let void_join = void_leaves.and_then(|place| self.void_leaf_join(&then, &els, ctx, place));
        let ty = if let Some(ty) = void_join {
            ty
        } else if nested {
            then.ty.clone()
        } else if self.joins_null_and_undefined(&then.ty, &els.ty) {
            // §195.1 rule 3: `tsc` types the conditional `T | null | undefined`,
            // with or without a contextual type (§195.11 rule 6).
            self.reject_both_spellings(pos.clone());
            Type::Error
        } else if self.involves_type_parameter(&then.ty) || self.involves_type_parameter(&els.ty) {
            self.generic_union(&then.ty, &els.ty)
        } else if let Some(context) = ctx {
            self.require_expr_assignable(&mut then, context, fx, "the then branch");
            self.require_expr_assignable(&mut els, context, fx, "the else branch");
            self.conditional_join(&then.ty, &els.ty)
                .unwrap_or_else(|| context.clone())
        } else {
            // compiler.md §248 rule 2: function branches take the type that
            // the other branch fits.
            let join = self.conditional_join(&then.ty, &els.ty).or_else(|| {
                leaves
                    .is_none()
                    .then(|| self.place_function_list(&mut [&mut then, &mut els], fx))
                    .flatten()
            });
            match join {
                Some(ty) => ty,
                None => {
                    let site = if self.apparent_type(&then.ty).is_numeric()
                        && self.apparent_type(&els.ty).is_numeric()
                    {
                        RejectionSite::ConditionalJoinSizedOperandWidths
                    } else {
                        RejectionSite::ConditionalJoinGeneralUnionAndUndefined
                    };
                    self.reject_subset(
                        site,
                        format!(
                            "conditional branches have no common type: `{}` and `{}`",
                            self.type_name(&then.ty),
                            self.type_name(&els.ty),
                        ),
                        pos.clone(),
                    );
                    Type::Error
                }
            }
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Cond {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            ty,
            pos,
        }
    }
}
