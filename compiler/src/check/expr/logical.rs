//! Checks short-circuit logical expressions (compiler.md §232 and §300).
use super::*;
use crate::check::{rejection::RejectionSite, FnCtx};
impl<'p> Checker<'p> {
    pub(super) fn check_logical(
        &mut self,
        b: &ast::LogicalExpression<'_>,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        let void_leaves = self.void_leaf_place.take();
        if b.operator == ast::LogicalOperator::Coalesce {
            return self.check_nullish(b, ctx, fx, pos);
        }
        // §232 rules 1 and 4: each operand of `||` and the right
        // operand of `&&` keep the context.
        let left_ctx = (b.operator == ast::LogicalOperator::Or)
            .then(|| self.operand_context(&b.left, ctx))
            .flatten();
        let left = self.check_expr(&b.left, left_ctx.as_ref(), fx);
        let right_ctx = self.operand_context(&b.right, ctx);
        let right = self.check_logical_right(&left, b, right_ctx.as_ref(), void_leaves, fx);
        // compiler.md §300 rules 1 and 2: only the left operand
        // is a truth test when the right operand is discarded.
        let discarded_right = void_leaves == Some(super::VoidLeafPlace::Statement);
        let sides: &[&hir::Expr] = if discarded_right {
            &[&left]
        } else {
            &[&left, &right]
        };
        for side in sides {
            if !self.instance_restriction(
                crate::check::opaque::InstanceRestriction::BooleanContext,
                &side.ty,
            ) && !matches!(self.apparent_type(&side.ty), Type::Bool | Type::Error)
            {
                let name = self.type_name(&side.ty);
                self.reject_subset(
                    RejectionSite::LogicalNonBooleanOperand,
                    format!("logical operators require booleans, got `{}`", name),
                    side.pos.clone(),
                );
            }
        }
        let ty = if discarded_right {
            Type::Void
        } else if self.involves_type_parameter(&left.ty) || self.involves_type_parameter(&right.ty)
        {
            if b.operator == ast::LogicalOperator::And
                && self.apparent_type(&(right.ty)) == Type::Bool
                && !matches!(self.apparent_type(&left.ty), Type::Nullable(_, _))
            {
                Type::Bool
            } else {
                self.generic_union(&left.ty, &right.ty)
            }
        } else {
            Type::Bool
        };
        let op = if b.operator == ast::LogicalOperator::And {
            BinOp::And
        } else {
            BinOp::Or
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
            pos,
        }
    }
}
