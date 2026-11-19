//! Unary operand checks keep validation outside recursive operand frames (§341 rule 6b).
use oxc_ast::ast;

use crate::check::rejection::RejectionSite;
use crate::check::{Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, ExprKind, UnOp};
use crate::types::Type;

impl<'p> Checker<'p> {
    pub(super) fn check_unary(
        &mut self,
        u: &ast::UnaryExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        match u.operator {
            ast::UnaryOperator::UnaryNegation => self.check_negation(u, ctx, fx, pos),
            ast::UnaryOperator::LogicalNot => self.check_logical_not(u, fx, pos),
            ast::UnaryOperator::BitwiseNot => self.check_bitwise_not(u, ctx, fx, pos),
            ast::UnaryOperator::Typeof
                if matches!(super::unparen_expr(&u.argument), ast::Expression::Identifier(id)
                    if !fx.owns_local_name(id.name.as_ref()) && matches!(self.type_scope_item(id.name.as_ref()), Some(ScopeItem::Namespace { .. }))) =>
            {
                self.check_expr(&u.argument, None, fx)
            }
            ast::UnaryOperator::Delete => {
                self.reject_subset(
                    RejectionSite::DeleteProperty,
                    "the `delete` operator is not in the language; use `Context.free`",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                self.reject_subset(
                    match u.operator {
                        ast::UnaryOperator::Typeof => {
                            if matches!(
                                super::unparen_expr(&u.argument),
                                ast::Expression::NumericLiteral(_)
                                    | ast::Expression::StringLiteral(_)
                                    | ast::Expression::BooleanLiteral(_)
                                    | ast::Expression::NullLiteral(_)
                                    | ast::Expression::BigIntLiteral(_)
                                    | ast::Expression::RegExpLiteral(_)
                            ) {
                                RejectionSite::TypeofLiteral
                            } else {
                                RejectionSite::TypeofOperator
                            }
                        }
                        ast::UnaryOperator::Void => RejectionSite::VoidOperator,
                        _ => RejectionSite::UnaryPlusOperator,
                    },
                    "unary operator outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    fn check_negation(
        &mut self,
        u: &ast::UnaryExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        // Fold `-literal` so the negative value is range-checked
        // against the contextual type (C4).
        let mut arg: &ast::Expression = &u.argument;
        while let ast::Expression::ParenthesizedExpression(p) = arg {
            arg = &p.expression;
        }
        if let ast::Expression::NumericLiteral(n) = arg {
            return self.check_num_lit(n, true, ctx, pos);
        }
        self.mark_operand_calls(&u.argument);
        let operand = self.check_expr(&u.argument, ctx, fx);
        self.finish_negation(operand, pos)
    }

    fn check_logical_not(
        &mut self,
        u: &ast::UnaryExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        let operand = self.check_expr(&u.argument, None, fx);
        self.finish_logical_not(operand, pos)
    }

    fn check_bitwise_not(
        &mut self,
        u: &ast::UnaryExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        self.mark_operand_calls(&u.argument);
        let operand = self.check_expr(&u.argument, ctx, fx);
        self.finish_bitwise_not(operand, pos)
    }

    fn finish_negation(&mut self, operand: hir::Expr, pos: Pos) -> hir::Expr {
        let apparent = self.apparent_type(&operand.ty);
        let parameter = self.instance_restriction(
            crate::check::opaque::InstanceRestriction::UnaryNumeric,
            &operand.ty,
        ) && !matches!(apparent, Type::Nullable(_, _));
        if !parameter && apparent == Type::F16 {
            self.reject_subset(
                RejectionSite::HalfFloatUnary,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if !parameter && !apparent.is_numeric() && !matches!(apparent, Type::Error) {
            let name = self.type_name(&operand.ty);
            self.reject_subset(
                RejectionSite::UnaryNumericCoercion,
                format!("unary `-` requires a numeric operand, got `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let ty = if parameter {
            Type::GenericNumber
        } else {
            operand.ty.clone()
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Unary {
                op: UnOp::Neg,
                operand: Box::new(operand),
            },
            ty,
            pos,
        }
    }

    fn finish_logical_not(&mut self, operand: hir::Expr, pos: Pos) -> hir::Expr {
        if !self.instance_restriction(
            crate::check::opaque::InstanceRestriction::BooleanContext,
            &operand.ty,
        ) && !matches!(self.apparent_type(&operand.ty), Type::Bool | Type::Error)
        {
            let name = self.type_name(&operand.ty);
            self.reject_subset(
                RejectionSite::LogicalNotNonBoolean,
                format!("`!` requires a boolean operand, got `{}`", name),
                pos.clone(),
            );
        }
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Unary {
                op: UnOp::Not,
                operand: Box::new(operand),
            },
            ty: Type::Bool,
            pos,
        }
    }

    fn finish_bitwise_not(&mut self, operand: hir::Expr, pos: Pos) -> hir::Expr {
        let apparent = self.apparent_type(&operand.ty);
        let parameter = self.instance_restriction(
            crate::check::opaque::InstanceRestriction::UnaryNumeric,
            &operand.ty,
        ) && !matches!(apparent, Type::Nullable(_, _));
        if !parameter && !apparent.is_integer() && !matches!(apparent, Type::Error) {
            let name = self.type_name(&operand.ty);
            self.reject_subset(
                RejectionSite::BitwiseIntegerOperand,
                format!("`~` requires an integer operand, got `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let ty = if parameter {
            Type::GenericNumber
        } else {
            operand.ty.clone()
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Unary {
                op: UnOp::BitNot,
                operand: Box::new(operand),
            },
            ty,
            pos,
        }
    }
}
