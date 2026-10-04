//! Checks the operator expressions, the conditional expression, `yield`, and `as`.

use crate::check::rejection::RejectionSite;
use std::collections::HashSet;
use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{static_member_symbol, Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, BinOp, Callee, ExprKind, UnOp};
use crate::types::Type;

use super::{
    assign_binary_op, flatten_optional_chain, is_place_expr, is_undefined_ident, literalish,
    opt_call_as_call, unparen_expr, write_spelling, BinResult, BinUse, OptionalPlan, OptionalStep,
    Place, PlaceSource, WriteExpression,
};

impl<'p> Checker<'p> {
    pub(super) fn check_unary(
        &mut self,
        u: &ast::UnaryExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        match u.op {
            ast::UnaryOp::Minus => {
                // Fold `-literal` so the negative value is range-checked
                // against the contextual type (C4).
                let mut arg: &ast::Expr = &u.arg;
                while let ast::Expr::Paren(p) = arg {
                    arg = &p.expr;
                }
                if let ast::Expr::Lit(ast::Lit::Num(n)) = arg {
                    return self.check_num_lit(n, true, ctx, pos);
                }
                let operand = self.check_expr(&u.arg, ctx, fx);
                let apparent = self.apparent_type(&operand.ty);
                let parameter = self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::UnaryNumeric,
                    &operand.ty,
                ) && !matches!(apparent, Type::Nullable(_));
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
            ast::UnaryOp::Bang => {
                let operand = self.check_expr(&u.arg, None, fx);
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
            ast::UnaryOp::Tilde => {
                let operand = self.check_expr(&u.arg, ctx, fx);
                let apparent = self.apparent_type(&operand.ty);
                let parameter = self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::UnaryNumeric,
                    &operand.ty,
                ) && !matches!(apparent, Type::Nullable(_));
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
            ast::UnaryOp::TypeOf
                if matches!(super::unparen_expr(&u.arg), ast::Expr::Ident(id)
                    if !fx.owns_local_name(id.sym.as_ref()) && matches!(self.type_scope_item(id.sym.as_ref()), Some(ScopeItem::Namespace { .. }))) =>
            {
                self.check_expr(&u.arg, None, fx)
            }
            ast::UnaryOp::Delete => {
                self.reject_subset(
                    RejectionSite::DeleteProperty,
                    "the `delete` operator is not in the language; use `Context.free`",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                self.reject_subset(
                    match u.op {
                        ast::UnaryOp::TypeOf => RejectionSite::TypeofOperator,
                        ast::UnaryOp::Void => RejectionSite::VoidOperator,
                        _ => RejectionSite::UnaryPlusOperator,
                    },
                    "unary operator outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    pub(super) fn stabilize_compound_operand(
        &mut self,
        expr: hir::Expr,
        label: &str,
        prefix: &mut crate::check::SyntheticPrefix,
    ) -> hir::Expr {
        if is_place_expr(&expr) {
            return expr;
        }
        let id = self.next_compound_local_id;
        self.next_compound_local_id += 1;
        let name = format!("[[compound#{id}.{label}]]");
        let ty = expr.ty.clone();
        let pos = expr.pos.clone();
        prefix.push(hir::Stmt::Let {
            name: name.clone(),
            ty: ty.clone(),
            mutable: false,
            dispose: false,
            init: expr,
            pos: pos.clone(),
        });
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Local(name, ty.clone()),
            ty,
            pos,
        }
    }

    pub(super) fn check_update(
        &mut self,
        u: &ast::UpdateExpr,
        fx: &mut FnCtx,
        pos: Pos,
        statement_position: bool,
        mut prefix: Option<&mut crate::check::SyntheticPrefix>,
    ) -> hir::Expr {
        let source = match unparen_expr(&u.arg) {
            ast::Expr::Ident(ident) => PlaceSource::Ident(ident),
            ast::Expr::Member(member) => PlaceSource::Member(member),
            _ => PlaceSource::Unsupported,
        };
        let place = self.check_assign_target(source, fx, &pos);
        #[cfg(test)]
        place.record_kind();
        if !statement_position {
            match &place {
                Place::IndexSignature { .. } => {
                    let spelling = write_spelling(WriteExpression::Update(u), "a[i]");
                    self.reject_subset(
                        RejectionSite::IndexUpdateExpressionValue,
                        format!("{spelling} cannot be used as a value"),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                Place::Accessor {
                    class,
                    receiver,
                    name,
                    ..
                } => {
                    let target = receiver.as_ref().map_or_else(
                        || format!("{}.{name}", self.classes[class.0].name),
                        |_| format!("x.{name}"),
                    );
                    let spelling = write_spelling(WriteExpression::Update(u), &target);
                    self.reject_subset(
                        RejectionSite::AccessorUpdateExpressionValue,
                        format!("{spelling} cannot be used as a value"),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                _ => {}
            }
        }
        let target_ty = self.apparent_type(place.ty());
        if !self.apparent_type(&target_ty).is_numeric()
            && !matches!(
                self.apparent_type(&target_ty),
                Type::GenericNumber | Type::Error
            )
        {
            let name = self.type_name(&target_ty);
            self.reject_subset(
                RejectionSite::UpdateNonNumericTarget,
                format!("`++`/`--` require a numeric target, got `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if target_ty == Type::F16 {
            self.reject_subset(
                RejectionSite::HalfFloatUpdate,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let op = if u.op == ast::UpdateOp::PlusPlus {
            BinOp::Add
        } else {
            BinOp::Sub
        };
        let one = hir::Expr {
            pending_work: None,
            kind: if target_ty.is_float() {
                ExprKind::Float(1.0)
            } else {
                ExprKind::Int(1)
            },
            ty: target_ty.clone(),
            pos: pos.clone(),
        };
        match place {
            Place::IndexSignature {
                receiver,
                index,
                signature,
                pos: target_pos,
            } => {
                if signature.readonly {
                    self.reject_subset(
                        RejectionSite::ReadonlyIndexUpdate,
                        "`a[i] = v` cannot write through a readonly index signature",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let prefix = prefix
                    .as_deref_mut()
                    .expect("a statement update must provide compound-write storage");
                let receiver = self.stabilize_compound_operand(receiver, "receiver", prefix);
                let index = self.stabilize_compound_operand(index, "index", prefix);
                let read = Place::IndexSignature {
                    receiver: receiver.clone(),
                    index: index.clone(),
                    signature: signature.clone(),
                    pos: target_pos,
                }
                .into_read(self);
                let binary_op = assign_binary_op(op).expect("an update operator is binary");
                let result = self.bin_result(
                    binary_op,
                    read,
                    one,
                    pos.clone(),
                    BinUse::CompoundAssignment,
                );
                if result.terminal {
                    return result.expr;
                }
                self.require_assignable(
                    &result.expr.ty.clone(),
                    &signature.element_ty,
                    result.expr.pos.clone(),
                    "the assignment",
                );
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Call {
                        callee: Callee::Method {
                            recv: Box::new(receiver),
                            name: hir::Symbol::from_full_text("set"),
                        },
                        args: vec![index, result.expr],
                    },
                    ty: Type::Void,
                    pos,
                }
            }
            Place::Accessor {
                class,
                receiver,
                name,
                ty,
                pos: target_pos,
            } => {
                let write_name = format!("{name}=");
                let signature = if receiver.is_some() {
                    self.class_sigs[class.0].methods.get(&write_name).cloned()
                } else {
                    self.class_sigs[class.0]
                        .static_methods
                        .get(&write_name)
                        .cloned()
                };
                let Some(signature) = signature else {
                    let target = receiver.as_ref().map_or_else(
                        || format!("{}.{name}", self.classes[class.0].name),
                        |_| format!("x.{name}"),
                    );
                    let spelling = write_spelling(WriteExpression::Update(u), &target);
                    self.reject_subset(
                        RejectionSite::ReadonlyAccessorUpdate,
                        format!("{spelling} cannot write through a read-only accessor"),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                let receiver = receiver.map(|receiver| {
                    let prefix =
                        prefix.expect("a statement update must provide compound-write storage");
                    self.stabilize_compound_operand(receiver, "receiver", prefix)
                });
                let read = Place::Accessor {
                    class,
                    receiver: receiver.clone(),
                    name: name.clone(),
                    ty,
                    pos: target_pos,
                }
                .into_read(self);
                let binary_op = assign_binary_op(op).expect("an update operator is binary");
                let result = self.bin_result(
                    binary_op,
                    read,
                    one,
                    pos.clone(),
                    BinUse::CompoundAssignment,
                );
                if result.terminal {
                    return result.expr;
                }
                let Some(parameter) = signature.params.first() else {
                    self.reject_subset(
                        RejectionSite::UpdateSetterParameterMissing,
                        format!("write accessor `{name}` has no parameter signature"),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                self.require_assignable(
                    &result.expr.ty.clone(),
                    parameter.ty(),
                    result.expr.pos.clone(),
                    "the assignment",
                );
                let callee = if let Some(receiver) = receiver {
                    Callee::Method {
                        recv: Box::new(receiver),
                        name: hir::Symbol::from_full_text(write_name),
                    }
                } else {
                    Callee::Func(hir::Symbol::from_full_text(static_member_symbol(
                        class,
                        &self.classes[class.0].name,
                        &write_name,
                    )))
                };
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Call {
                        callee,
                        args: vec![result.expr],
                    },
                    ty: Type::Void,
                    pos,
                }
            }
            place => {
                let target = place.into_read(self);
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Assign {
                        update: Some(if u.prefix {
                            hir::UpdateKind::Prefix
                        } else {
                            hir::UpdateKind::Postfix
                        }),
                        op: Some(op),
                        target: Box::new(target),
                        value: Box::new(one),
                    },
                    ty: target_ty,
                    pos,
                }
            }
        }
    }

    pub(super) fn check_bin(
        &mut self,
        b: &ast::BinExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        use ast::BinaryOp as B;
        if matches!(b.op, B::EqEq | B::NotEq | B::EqEqEq | B::NotEqEq) {
            if let Some(presence) = self.check_absence_presence_comparison(b, fx, pos.clone()) {
                return presence;
            }
        }
        match b.op {
            B::LogicalAnd | B::LogicalOr => {
                let left = self.check_expr(&b.left, None, fx);
                let right = self.check_logical_right(&left, b, fx);
                for side in [&left, &right] {
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
                let ty = if self.involves_type_parameter(&left.ty)
                    || self.involves_type_parameter(&right.ty)
                {
                    if b.op == B::LogicalAnd
                        && self.apparent_type(&(right.ty)) == Type::Bool
                        && !matches!(self.apparent_type(&left.ty), Type::Nullable(_))
                    {
                        Type::Bool
                    } else {
                        self.generic_union(&left.ty, &right.ty)
                    }
                } else {
                    Type::Bool
                };
                let op = if b.op == B::LogicalAnd {
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
            B::NullishCoalescing => self.check_nullish(b, fx, pos),
            B::InstanceOf => self.check_instanceof(b, fx, pos),
            B::In | B::Exp => {
                self.reject_subset(
                    if b.op == B::In {
                        RejectionSite::InOperator
                    } else {
                        RejectionSite::ExponentOperator
                    },
                    "operator outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                let arith = matches!(b.op, B::Add | B::Sub | B::Mul | B::Div | B::Mod);
                let outer: Option<Type> = if arith { ctx.cloned() } else { None };
                let (mut left, mut right);
                if literalish(&b.left) && !literalish(&b.right) {
                    let r = self.check_expr(&b.right, outer.as_ref(), fx);
                    let c = self.literal_context(&r.ty).or(outer);
                    left = self.check_expr(&b.left, c.as_ref(), fx);
                    right = r;
                } else {
                    left = self.check_expr(&b.left, outer.as_ref(), fx);
                    let c = if literalish(&b.right) {
                        self.literal_context(&left.ty).or(outer)
                    } else {
                        outer
                    };
                    right = self.check_expr(&b.right, c.as_ref(), fx);
                }
                if matches!(b.op, B::EqEq | B::NotEq | B::EqEqEq | B::NotEqEq) {
                    if self.apparent_type(&right.ty) == Type::Null {
                        self.restore_nullable_storage(&mut left);
                    }
                    if self.apparent_type(&left.ty) == Type::Null {
                        self.restore_nullable_storage(&mut right);
                    }
                }
                self.bin_result(b.op, left, right, pos, BinUse::Expression)
                    .expr
            }
        }
    }

    fn restore_nullable_storage(&self, expression: &mut hir::Expr) {
        let target = match &expression.kind {
            ExprKind::Assign {
                op: None, target, ..
            } => target.as_ref(),
            _ => expression,
        };
        let storage = if let ExprKind::Global(name) = &target.kind {
            self.globals
                .iter()
                .find(|global| global.symbol == *name)
                .map(|global| &global.ty)
        } else {
            Some(target.storage_type(&self.classes))
        };
        if let Some(storage) =
            storage.filter(|ty| matches!(self.apparent_type(ty), Type::Nullable(_)))
        {
            expression.ty = storage.clone();
        }
    }

    pub(super) fn reject_unbound_optional_chain(
        &mut self,
        chain: &ast::OptChainExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let plan = self.check_optional_plan(chain, fx);
        if self.apparent_type(&(plan.value.ty)) == Type::Error {
            return self.err_expr(pos);
        }
        let name = self.type_name(&plan.value.ty);
        self.reject_subset(RejectionSite::OptionalCallValueWithoutFallback, format!(
                "an optional chain has type `{name} | undefined` in TypeScript; give it a fallback with `??` or use it as a statement"
            ), pos.clone());
        self.err_expr(pos)
    }

    pub(super) fn check_optional_chain_statement(
        &mut self,
        chain: &ast::OptChainExpr,
        fx: &mut FnCtx,
    ) -> Vec<hir::Stmt> {
        let pos = self.pos(chain.span);
        let plan = self.check_optional_plan(chain, fx);
        let mut out = Vec::new();
        if self.apparent_type(&(plan.value.ty)) == Type::Error {
            out.push(hir::Stmt::Expr(self.err_expr(pos)));
            return out;
        }
        if !plan.ends_in_call {
            let name = self.type_name(&plan.value.ty);
            self.reject_subset(RejectionSite::OptionalMemberValueWithoutFallback, format!(
                    "an optional chain has type `{name} | undefined` in TypeScript; give it a fallback with `??` or use it as a statement"
                ), pos.clone());
            out.push(hir::Stmt::Expr(self.err_expr(pos)));
            return out;
        }

        let mut body = vec![hir::Stmt::Expr(plan.value)];
        for test in plan.tests.into_iter().rev() {
            body = vec![hir::Stmt::If {
                cond: test,
                then: body,
                els: None,
                pos: pos.clone(),
            }];
        }
        out.extend(body);
        out
    }

    fn check_optional_plan(&mut self, chain: &ast::OptChainExpr, fx: &mut FnCtx) -> OptionalPlan {
        let mut steps = Vec::new();
        let root = flatten_optional_chain(chain, &mut steps);
        let mut current = self.check_expr(root, None, fx);
        let mut tests = Vec::new();
        let mut ends_in_call = false;
        let mut index = 0;

        if self.apparent_type(&(current.ty)) == Type::Error {
            return OptionalPlan {
                tests,
                value: current,
                ends_in_call,
            };
        }

        while index < steps.len() {
            match &steps[index] {
                OptionalStep::Member { member, tested } => {
                    if *tested && matches!(member.prop, ast::MemberProp::Computed(_)) {
                        self.reject_subset(RejectionSite::OptionalComputedMember, "an optional chain cannot use `?.[i]`; narrow the receiver and use `[i]`", self.pos(member.span));
                        return OptionalPlan {
                            tests,
                            value: self.err_expr(self.pos(member.span)),
                            ends_in_call: false,
                        };
                    }
                    if *tested {
                        self.restore_nullable_storage(&mut current);
                        let Some(inner) = self.require_nullable_operand(
                            &current,
                            "the tested receiver",
                            RejectionSite::OptionalReceiverNonNullable,
                        ) else {
                            return OptionalPlan {
                                tests,
                                value: self.err_expr(current.pos.clone()),
                                ends_in_call: false,
                            };
                        };
                        let (test, value) = self.stabilize_nullable_operand(current, inner, fx);
                        tests.push(test);
                        current = value;
                    }

                    if let Some(OptionalStep::Call {
                        call,
                        tested: call_tested,
                    }) = steps.get(index + 1)
                    {
                        if *call_tested {
                            self.reject_subset(
                                RejectionSite::OptionalMethodCall,
                                "an optional call through `?.()` is not in the decided surface",
                                self.pos(call.span),
                            );
                            return OptionalPlan {
                                tests,
                                value: self.err_expr(self.pos(call.span)),
                                ends_in_call: false,
                            };
                        }
                        let ast::MemberProp::Ident(property) = &member.prop else {
                            current = self.check_optional_member(current, member, fx);
                            index += 1;
                            ends_in_call = false;
                            continue;
                        };
                        let call = opt_call_as_call(call);
                        current = self.check_method_call_on(
                            current,
                            property,
                            &call,
                            fx,
                            self.pos(call.span),
                        );
                        index += 2;
                        self.end_shared_narrowing(&current, fx);
                        ends_in_call = true;
                        continue;
                    }

                    current = self.check_optional_member(current, member, fx);
                    index += 1;
                    ends_in_call = false;
                }
                OptionalStep::Call { call, tested } => {
                    if *tested {
                        self.reject_subset(
                            RejectionSite::OptionalFunctionCall,
                            "an optional call through `?.()` is not in the decided surface",
                            self.pos(call.span),
                        );
                        return OptionalPlan {
                            tests,
                            value: self.err_expr(self.pos(call.span)),
                            ends_in_call: false,
                        };
                    }
                    let call = opt_call_as_call(call);
                    current = self.check_indirect_call(current, &call, fx, self.pos(call.span));
                    self.end_shared_narrowing(&current, fx);
                    index += 1;
                    ends_in_call = true;
                }
            }
        }

        OptionalPlan {
            tests,
            value: current,
            ends_in_call,
        }
    }

    fn check_optional_member(
        &mut self,
        receiver: hir::Expr,
        member: &ast::MemberExpr,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        match &member.prop {
            ast::MemberProp::Ident(property) => self.member_on(
                receiver,
                property.sym.as_ref(),
                self.pos(property.span),
                None,
                fx,
            ),
            ast::MemberProp::Computed(property) => {
                let index_context = match &self.apparent_type(&receiver.ty) {
                    Type::Class(id) => self.classes[id.0]
                        .index_signature
                        .as_ref()
                        .map(|signature| signature.index_ty.clone())
                        .unwrap_or(Type::I32),
                    _ => Type::I32,
                };
                let index = self.check_expr(&property.expr, Some(&index_context), fx);
                self.check_index(receiver, index, self.pos(member.span))
            }
            ast::MemberProp::PrivateName(_) => {
                let pos = self.pos(member.span);
                self.reject_subset(
                    RejectionSite::OptionalPrivateMember,
                    "private names are not in the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    fn check_nullish(&mut self, binary: &ast::BinExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let left_ast = unparen_expr(&binary.left);
        if let ast::Expr::OptChain(chain) = left_ast {
            let plan = self.check_optional_plan(chain, fx);
            return self.finish_nullish_plan(plan, &binary.right, fx, pos);
        }

        let mut left = self.check_expr(&binary.left, None, fx);
        self.restore_nullable_storage(&mut left);
        if matches!(left_ast, ast::Expr::Bin(binary) if binary.op == ast::BinaryOp::NullishCoalescing)
            && !matches!(self.apparent_type(&left.ty), Type::Nullable(_))
            && self.allows_nullable(&left.ty)
        {
            // A preceding fallback can have a narrowed result type.
            left.ty = Type::nullable(left.ty);
        }
        if self.apparent_type(&(left.ty)) == Type::Error {
            return self.err_expr(pos);
        }
        if self.involves_type_parameter(&left.ty) {
            let right = self.check_expr(&binary.right, None, fx);
            let inner = self.non_null_type(&left.ty);
            let ty = if matches!(self.apparent_type(&left.ty), Type::Nullable(_))
                || self.is_unconstrained_type_parameter(&left.ty)
            {
                self.generic_union(&inner, &right.ty)
            } else {
                left.ty.clone()
            };
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                ty,
                pos,
            };
        }
        let Some(inner) = self.require_nullable_operand(
            &left,
            "the left operand of `??`",
            RejectionSite::NullishReceiverNonNullable,
        ) else {
            return self.err_expr(pos);
        };
        let (test, value) = self.stabilize_nullable_operand(left, inner.clone(), fx);
        let right = self.check_expr(&binary.right, Some(&inner), fx);
        let result_ty = self.nullish_result_type(&inner, &right, "the right operand");
        self.render_nullish_cond(vec![test], value, right, result_ty, pos)
    }

    fn finish_nullish_plan(
        &mut self,
        mut plan: OptionalPlan,
        right_ast: &ast::Expr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if self.apparent_type(&(plan.value.ty)) == Type::Error {
            return self.err_expr(pos);
        }
        let value_ty = plan.value.ty.clone();
        match self.apparent_type(&value_ty) {
            Type::Nullable(inner) => {
                let (test, value) =
                    self.stabilize_nullable_operand(plan.value, (*inner).clone(), fx);
                plan.tests.push(test);
                let right = self.check_expr(right_ast, Some(&inner), fx);
                let result_ty = self.nullish_result_type(&inner, &right, "the right operand");
                self.render_nullish_cond(plan.tests, value, right, result_ty, pos)
            }
            value_ty => {
                let right = self.check_expr(right_ast, Some(&value_ty), fx);
                self.require_assignable(
                    &right.ty.clone(),
                    &value_ty,
                    right.pos.clone(),
                    "the right operand",
                );
                self.render_nullish_cond(plan.tests, plan.value, right, value_ty, pos)
            }
        }
    }

    fn nullish_result_type(&mut self, inner: &Type, right: &hir::Expr, what: &str) -> Type {
        if right.ty == *inner {
            return inner.clone();
        }
        let nullable = Type::nullable(inner.clone());
        if right.ty == nullable {
            return nullable;
        }
        if self.apparent_type(&(right.ty)) != Type::Error {
            self.require_assignable(&right.ty.clone(), inner, right.pos.clone(), what);
        }
        Type::Error
    }

    fn require_nullable_operand(
        &mut self,
        operand: &hir::Expr,
        what: &str,
        site: RejectionSite,
    ) -> Option<Type> {
        if let Type::Nullable(_) = self.apparent_type(&operand.ty) {
            return Some(self.non_null_type(&operand.ty));
        }
        let name = self.type_name(&operand.ty);
        self.reject_subset(
            site,
            format!("{what} has type `{name}`, which is not nullable"),
            operand.pos.clone(),
        );
        None
    }

    fn stabilize_nullable_operand(
        &mut self,
        operand: hir::Expr,
        inner: Type,
        fx: &mut FnCtx,
    ) -> (hir::Expr, hir::Expr) {
        let pos = operand.pos.clone();
        if is_place_expr(&operand) {
            let mut value = operand.clone();
            value.ty = inner;
            return (self.null_test(operand, pos), value);
        }

        let id = self.next_compound_local_id;
        self.next_compound_local_id += 1;
        let name = format!("[[compound#{id}.nullish]]");
        let nullable = operand.ty.clone();
        fx.declare(
            &name,
            crate::check::Local {
                ty: nullable.clone(),
                mutable: true,
                async_origins: std::collections::HashSet::new(),
                caught: false,
            },
        );
        fx.push_synthetic_prefix(hir::Stmt::Let {
            name: name.clone(),
            ty: nullable.clone(),
            mutable: true,
            dispose: false,
            init: hir::Expr {
                pending_work: None,
                kind: ExprKind::Null,
                ty: Type::Null,
                pos: pos.clone(),
            },
            pos: pos.clone(),
        });
        let target = hir::Expr {
            pending_work: None,
            kind: ExprKind::Local(name.clone(), nullable.clone()),
            ty: nullable.clone(),
            pos: pos.clone(),
        };
        let assigned = hir::Expr {
            pending_work: None,
            kind: ExprKind::Assign {
                update: None,
                op: None,
                target: Box::new(target),
                value: Box::new(operand),
            },
            ty: nullable.clone(),
            pos: pos.clone(),
        };
        let value = hir::Expr {
            pending_work: None,
            kind: ExprKind::Local(name, nullable),
            ty: inner,
            pos: pos.clone(),
        };
        (self.null_test(assigned, pos), value)
    }

    fn null_test(&self, value: hir::Expr, pos: Pos) -> hir::Expr {
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(value),
                right: Box::new(hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Null,
                    ty: Type::Null,
                    pos: pos.clone(),
                }),
            },
            ty: Type::Bool,
            pos,
        }
    }

    fn render_nullish_cond(
        &self,
        tests: Vec<hir::Expr>,
        value: hir::Expr,
        fallback: hir::Expr,
        ty: Type,
        pos: Pos,
    ) -> hir::Expr {
        tests.into_iter().rev().fold(value, |then, cond| hir::Expr {
            pending_work: None,
            kind: ExprKind::Cond {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(fallback.clone()),
            },
            ty: ty.clone(),
            pos: pos.clone(),
        })
    }

    /// Checks the sole legal `undefined` appearance (§43).
    fn check_absence_presence_comparison(
        &mut self,
        binary: &ast::BinExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> Option<hir::Expr> {
        let left_undefined = is_undefined_ident(&binary.left);
        let right_undefined = is_undefined_ident(&binary.right);
        if !left_undefined && !right_undefined {
            return None;
        }

        let undefined_source = if left_undefined {
            &*binary.left
        } else {
            &*binary.right
        };
        if left_undefined && right_undefined {
            self.reject_subset(RejectionSite::UndefinedEqualityPair, "`undefined` is legal only in a presence test on an absence-capable descriptor member", self.pos(undefined_source.span()));
            return Some(self.err_expr(pos));
        }

        let member_source = if left_undefined {
            &*binary.right
        } else {
            &*binary.left
        };
        let checked = match unparen_expr(member_source) {
            ast::Expr::Member(member) => self.check_member_read_inner(member, fx, true, false),
            other => self.check_expr(other, None, fx),
        };
        if !self.is_absence_capable_member_expr(&checked) {
            self.reject_subset(RejectionSite::UndefinedEqualityNonMember, "`undefined` is legal only in a presence test on an absence-capable descriptor member", self.pos(undefined_source.span()));
            return Some(self.err_expr(pos));
        }

        Some(hir::Expr {
            pending_work: None,
            kind: ExprKind::AbsenceTest {
                value: Box::new(checked),
                negated: matches!(binary.op, ast::BinaryOp::NotEq | ast::BinaryOp::NotEqEq),
            },
            ty: Type::Bool,
            pos,
        })
    }

    pub(super) fn bin_result(
        &mut self,
        op: ast::BinaryOp,
        left: hir::Expr,
        right: hir::Expr,
        pos: Pos,
        use_kind: BinUse,
    ) -> BinResult {
        use ast::BinaryOp as B;
        let equality_operator = match op {
            B::EqEq => Some("=="),
            B::NotEq => Some("!="),
            B::EqEqEq => Some("==="),
            B::NotEqEq => Some("!=="),
            _ => None,
        };
        let lt = self.apparent_type(&left.ty);
        let rt = self.apparent_type(&right.ty);
        let parameter =
            self.involves_type_parameter(&left.ty) || self.involves_type_parameter(&right.ty);
        let numeric = |ty: &Type| {
            self.apparent_type(ty).is_numeric()
                || matches!(&self.apparent_type(ty), Type::GenericNumber)
                || (parameter && matches!(self.apparent_type(ty), Type::Enum(_)))
        };
        let numeric_pair = numeric(&lt)
            && numeric(&rt)
            && (lt == rt
                || self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::SizedNumeric,
                    &left.ty,
                )
                || self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::SizedNumeric,
                    &right.ty,
                )
                || matches!(lt, Type::GenericNumber)
                || matches!(rt, Type::GenericNumber));
        let integer_pair = numeric_pair
            && (self.instance_restriction(
                crate::check::opaque::InstanceRestriction::SizedNumeric,
                &left.ty,
            ) || self.instance_restriction(
                crate::check::opaque::InstanceRestriction::SizedNumeric,
                &right.ty,
            ) || ((lt.is_integer() || matches!(lt, Type::GenericNumber))
                && (rt.is_integer() || matches!(rt, Type::GenericNumber))));
        // §143 rule 1a: a parameter compared with itself has overlapping values.
        let same_parameter = left.ty == right.ty && parameter;
        let related_parameter = self.generic_overlap(&left.ty, &right.ty);
        let operand_error = matches!(lt, Type::Error) || matches!(rt, Type::Error);
        let suppress_error = match use_kind {
            BinUse::Expression => operand_error,
            BinUse::CompoundAssignment => matches!(lt, Type::Error),
        };
        let mixed_numeric = numeric(&lt) && numeric(&rt) && lt != rt;
        let arithmetic = matches!(op, B::Add | B::Sub | B::Mul | B::Div | B::Mod);
        let left_storage_only = lt == Type::F16
            && !self.instance_restriction(
                crate::check::opaque::InstanceRestriction::SizedNumeric,
                &left.ty,
            );
        let right_storage_only = rt == Type::F16
            && !self.instance_restriction(
                crate::check::opaque::InstanceRestriction::SizedNumeric,
                &right.ty,
            );
        let f16_arithmetic = arithmetic
            && match use_kind {
                BinUse::Expression => left_storage_only || right_storage_only,
                BinUse::CompoundAssignment => left_storage_only,
            };
        if f16_arithmetic {
            self.reject_subset(
                RejectionSite::HalfFloatBinary,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
            );
            return BinResult {
                expr: self.err_expr(pos),
                terminal: true,
            };
        }
        let mk = |op: BinOp, ty: Type| hir::Expr {
            pending_work: None,
            kind: ExprKind::Binary {
                op,
                left: Box::new(left.clone()),
                right: Box::new(right.clone()),
            },
            ty,
            pos: pos.clone(),
        };
        let (hop, ty, ok) = match op {
            B::Add => {
                if (parameter
                    && (lt == Type::Str || rt == Type::Str)
                    && self.instance_restriction(
                        crate::check::opaque::InstanceRestriction::TemplateInterpolation,
                        if lt == Type::Str { &right.ty } else { &left.ty },
                    ))
                    || (use_kind == BinUse::CompoundAssignment && numeric(&lt) && rt == Type::Str)
                    || (lt == Type::Str && rt == Type::Str)
                {
                    (BinOp::Add, Type::Str, true)
                } else if numeric_pair {
                    (BinOp::Add, lt.clone(), true)
                } else {
                    (BinOp::Add, Type::Error, suppress_error)
                }
            }
            B::Sub | B::Mul | B::Div | B::Mod => {
                let hop = match op {
                    B::Sub => BinOp::Sub,
                    B::Mul => BinOp::Mul,
                    B::Div => BinOp::Div,
                    _ => BinOp::Rem,
                };
                if numeric_pair {
                    (hop, lt.clone(), true)
                } else {
                    (hop, Type::Error, suppress_error)
                }
            }
            B::Lt | B::LtEq | B::Gt | B::GtEq => {
                let hop = match op {
                    B::Lt => BinOp::Lt,
                    B::LtEq => BinOp::Le,
                    B::Gt => BinOp::Gt,
                    _ => BinOp::Ge,
                };
                let comparable = numeric_pair || (matches!(lt, Type::Enum(_)) && lt == rt);
                (
                    hop,
                    Type::Bool,
                    comparable
                        || (parameter
                            && !numeric(&lt)
                            && !numeric(&rt)
                            && !matches!(lt, Type::Nullable(_))
                            && !matches!(rt, Type::Nullable(_))
                            && self.generic_overlap(&left.ty, &right.ty)
                            && self.instance_restriction(
                                crate::check::opaque::InstanceRestriction::RelationalKind,
                                if self.involves_type_parameter(&left.ty) {
                                    &left.ty
                                } else {
                                    &right.ty
                                },
                            ))
                        || (same_parameter
                            && !matches!(self.apparent_type(&left.ty), Type::Nullable(_)))
                        || (parameter
                            && ((self.is_unconstrained_type_parameter(&left.ty)
                                && matches!(rt, Type::Str | Type::Bool))
                                || (self.is_unconstrained_type_parameter(&right.ty)
                                    && matches!(lt, Type::Str | Type::Bool)))),
                )
            }
            B::EqEq | B::NotEq | B::EqEqEq | B::NotEqEq => {
                let hop = if matches!(op, B::EqEq | B::EqEqEq) {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                };
                let null_cmp = matches!(
                    (&lt, &rt),
                    (Type::Null, Type::Nullable(_))
                        | (Type::Nullable(_), Type::Null)
                        | (Type::Null, Type::Null)
                );
                let same_scalar = lt == rt
                    && (lt.is_numeric()
                        || matches!(lt, Type::Bool | Type::Str | Type::Enum(_))
                        || matches!(lt, Type::StringAlias(_))
                        || self.is_reference_class(&lt));
                (
                    hop,
                    Type::Bool,
                    if parameter {
                        related_parameter
                    } else {
                        null_cmp || same_scalar
                    },
                )
            }
            B::BitAnd | B::BitOr | B::BitXor | B::LShift | B::RShift | B::ZeroFillRShift => {
                let hop = match op {
                    B::BitAnd => BinOp::BitAnd,
                    B::BitOr => BinOp::BitOr,
                    B::BitXor => BinOp::BitXor,
                    B::LShift => BinOp::Shl,
                    B::RShift => BinOp::Shr,
                    _ => BinOp::UShr,
                };
                if integer_pair {
                    (hop, lt.clone(), true)
                } else {
                    (hop, Type::Error, suppress_error)
                }
            }
            _ => (BinOp::Add, Type::Error, suppress_error),
        };
        if ok || suppress_error {
            return BinResult {
                expr: mk(
                    hop,
                    if operand_error {
                        Type::Error
                    } else if parameter && numeric(&ty) {
                        Type::GenericNumber
                    } else {
                        ty
                    },
                ),
                terminal: false,
            };
        }
        if use_kind == BinUse::CompoundAssignment && !mixed_numeric {
            let operator = match op {
                B::Add => "+=",
                B::Sub => "-=",
                B::Mul => "*=",
                B::Div => "/=",
                B::Mod => "%=",
                B::BitAnd => "&=",
                B::BitOr => "|=",
                B::BitXor => "^=",
                B::LShift => "<<=",
                B::RShift => ">>=",
                B::ZeroFillRShift => ">>>=",
                _ => "compound assignment",
            };
            self.reject_subset(
                if matches!((&lt, &rt), (Type::Enum(_), _) | (_, Type::Enum(_))) {
                    RejectionSite::CompoundEnumOperand
                } else if lt == Type::Str || rt == Type::Str {
                    RejectionSite::CompoundStringOperand
                } else {
                    RejectionSite::CompoundInvalidOperand
                },
                format!(
                    "operator `{operator}` is not defined for `{}` and `{}`",
                    self.type_name(&lt),
                    self.type_name(&rt)
                ),
                pos.clone(),
            );
            return BinResult {
                expr: self.err_expr(pos),
                terminal: false,
            };
        }
        // Q20: Dates are values erasing to i64, but the nominal wall
        // stands both ways — comparison crosses through `getTime()`.
        if lt == Type::Date
            && rt == Type::Date
            && matches!(
                op,
                B::EqEq | B::NotEq | B::EqEqEq | B::NotEqEq | B::Lt | B::LtEq | B::Gt | B::GtEq
            )
        {
            self.reject_api_form(
                "Date",
                "direct comparison",
                &equality_operator.map_or_else(
                    || "Date direct comparison".to_string(),
                    |operator| format!("Date direct comparison `{operator}`"),
                ),
                pos.clone(),
            );
            return BinResult {
                expr: self.err_expr(pos),
                terminal: false,
            };
        }
        let ln = self.type_name(&left.ty);
        let rn = self.type_name(&right.ty);
        if mixed_numeric {
            let family = if let Some(operator) = equality_operator {
                format!("comparison `{operator}`")
            } else if matches!(
                op,
                B::BitAnd | B::BitOr | B::BitXor | B::LShift | B::RShift | B::ZeroFillRShift
            ) {
                "bitwise".to_string()
            } else {
                "arithmetic".to_string()
            };
            self.reject_subset(
                RejectionSite::BinaryMixedNumericTypes,
                format!(
                    "mixed-type {} (`{}` and `{}`) requires an explicit `as` conversion",
                    family, ln, rn
                ),
                pos.clone(),
            );
        } else {
            self.reject_subset(
                if equality_operator.is_some() && (lt == Type::Null || rt == Type::Null) {
                    RejectionSite::NonNullableNullEquality
                } else if matches!((&lt, &rt), (Type::Enum(_), _) | (_, Type::Enum(_))) {
                    if equality_operator.is_some()
                        && matches!((&lt, &rt), (Type::Enum(a), Type::Enum(b)) if a != b)
                    {
                        RejectionSite::BinaryInvalidOperand
                    } else if (matches!(lt, Type::Enum(_)) && rt.is_numeric())
                        || (matches!(rt, Type::Enum(_)) && lt.is_numeric())
                        || matches!((&lt, &rt), (Type::Enum(_), Type::Enum(_)))
                        || (op == B::Add && (lt == Type::Str || rt == Type::Str))
                    {
                        RejectionSite::BinaryEnumOperand
                    } else {
                        RejectionSite::BinaryInvalidOperand
                    }
                } else if lt == Type::Str
                    && rt == Type::Str
                    && matches!(op, B::Lt | B::LtEq | B::Gt | B::GtEq)
                {
                    RejectionSite::StringRelationalOperand
                } else if lt == Type::Str || rt == Type::Str {
                    if op == B::Add {
                        RejectionSite::BinaryStringOperand
                    } else {
                        RejectionSite::BinaryInvalidOperand
                    }
                } else if lt == Type::Bool
                    && rt == Type::Bool
                    && matches!(op, B::Lt | B::LtEq | B::Gt | B::GtEq)
                {
                    RejectionSite::BooleanRelationalOperand
                } else if equality_operator.is_some()
                    && (self.ts_erased_assignable(&lt, &rt) || self.ts_erased_assignable(&rt, &lt))
                {
                    RejectionSite::ErasedAssignableEquality
                } else {
                    RejectionSite::BinaryInvalidOperand
                },
                equality_operator.map_or_else(
                    || format!("operator not defined for `{ln}` and `{rn}`"),
                    |operator| format!("operator `{operator}` not defined for `{ln}` and `{rn}`"),
                ),
                pos.clone(),
            );
        }
        BinResult {
            expr: self.err_expr(pos),
            terminal: false,
        }
    }

    /// Checks the right operand of `&&` with the facts that hold when the
    /// left operand is true, and of `||` with the facts that hold when it
    /// is false (compiler.md §133). A kill inside the operand ends a fact
    /// (§124), and the facts end with the operand.
    fn check_logical_right(
        &mut self,
        left: &hir::Expr,
        b: &ast::BinExpr,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let note_paths = fx.narrowing_note_paths();
        let (when_true, when_false) = self.narrowing_paths(left, fx);
        let extra = if b.op == ast::BinaryOp::LogicalAnd {
            when_true
        } else {
            when_false
        };
        let mut base = fx.narrowed.clone();
        fx.narrowed = base.iter().cloned().chain(extra).collect();
        let right = self.check_expr(&b.right, None, fx);
        // Keep kills: facts removed inside the operand stay removed.
        base.retain(|key| fx.narrowed.contains(key));
        fx.narrowed = base;
        fx.finish_narrowing_join(&note_paths);
        right
    }

    // C4 and Q32: a literal can adopt a numeric or string-alias context.
    fn literal_context(&self, ty: &Type) -> Option<Type> {
        (self.apparent_type(ty).is_numeric()
            || matches!(self.apparent_type(ty), Type::StringAlias(_)))
        .then(|| ty.clone())
    }

    // §146: equal types and legal nullable pairs have a symmetric join.
    fn conditional_join(&self, left: &Type, right: &Type) -> Option<Type> {
        if matches!(left, Type::Error) || matches!(right, Type::Error) {
            return Some(Type::Error);
        }
        if left == right {
            return Some(left.clone());
        }
        for (value, other) in [(left, right), (right, left)] {
            if let Type::Nullable(inner) = value {
                if (other == inner.as_ref() || matches!(other, Type::Null))
                    && self.allows_nullable(inner)
                {
                    return Some(value.clone());
                }
            }
            if matches!(other, Type::Null) && self.allows_nullable(value) {
                return Some(Type::nullable(value.clone()));
            }
        }
        None
    }

    pub(super) fn check_cond(
        &mut self,
        c: &ast::CondExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
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
        let first = self.check_expr(first_ast, ctx, fx);
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
        let second = self.check_expr(second_ast, ctx.or(literal_ctx.as_ref()), fx);
        let first_possible: HashSet<_> = first_facts.union(&first_notes).cloned().collect();
        let second_possible: HashSet<_> = fx
            .narrowed
            .union(&fx.ended_shared_narrowing)
            .cloned()
            .collect();
        note_paths.extend(first_possible.intersection(&second_possible).cloned());
        let joined = first_facts.intersection(&fx.narrowed).cloned().collect();
        let (then, els) = if reverse {
            (second, first)
        } else {
            (first, second)
        };
        fx.narrowed = joined;
        fx.ended_shared_narrowing.extend(first_notes);
        fx.finish_narrowing_join(&note_paths);

        let ty = if self.involves_type_parameter(&then.ty) || self.involves_type_parameter(&els.ty)
        {
            self.generic_union(&then.ty, &els.ty)
        } else if let Some(context) = ctx {
            self.require_assignable(
                &then.ty.clone(),
                context,
                then.pos.clone(),
                "the then branch",
            );
            self.require_assignable(&els.ty.clone(), context, els.pos.clone(), "the else branch");
            self.conditional_join(&then.ty, &els.ty)
                .unwrap_or_else(|| context.clone())
        } else {
            match self.conditional_join(&then.ty, &els.ty) {
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

    pub(super) fn check_yield(
        &mut self,
        y: &ast::YieldExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let (in_generator, known_yield) = fx
            .frames
            .last()
            .map(|f| (f.is_generator, f.yield_ty.clone()))
            .unwrap_or((false, None));
        if !in_generator {
            self.reject_subset(
                if fx.frames.last().is_some_and(|frame| frame.is_async) {
                    RejectionSite::AsyncGeneratorYield
                } else {
                    RejectionSite::YieldOutsideGenerator
                },
                "`yield` is only available inside a `function*` coroutine",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if y.delegate {
            self.reject_subset(
                RejectionSite::YieldDelegation,
                "`yield*` delegation is not in the decided surface",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let arg = match &y.arg {
            Some(arg) => {
                let e = self.check_expr(arg, known_yield.as_ref(), fx);
                match &known_yield {
                    Some(t) => {
                        self.require_assignable(&e.ty.clone(), &t.clone(), e.pos.clone(), "yield")
                    }
                    None => {
                        if let Some(frame) = fx.frames.last_mut() {
                            frame.yield_ty = Some(e.ty.clone());
                        }
                    }
                }
                Some(Box::new(e))
            }
            None => {
                if let Some(ty) = &known_yield {
                    if !matches!(self.apparent_type(ty), Type::Void | Type::Error) {
                        let name = self.type_name(ty);
                        self.reject_subset(if fx.frames.last().is_some_and(|frame| frame.yield_annotated) { RejectionSite::BareYieldDeclaredNonVoid } else { RejectionSite::BareYieldNonVoid }, format!("a bare `yield;` requires `void`, but the generator element type is `{name}`"), pos.clone());
                    }
                } else if let Some(frame) = fx.frames.last_mut() {
                    frame.yield_ty = Some(Type::Void);
                }
                None
            }
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Yield(arg),
            ty: Type::Void,
            pos,
        }
    }

    pub(super) fn check_as(&mut self, a: &ast::TsAsExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let target = self.resolve_type(&a.type_ann);
        let inner = self.check_expr(&a.expr, None, fx);
        let src = inner.ty.clone();
        let generic_cast = (self
            .instance_restriction(crate::check::opaque::InstanceRestriction::CastKind, &src)
            || self.instance_restriction(
                crate::check::opaque::InstanceRestriction::CastKind,
                &target,
            ))
            && self.generic_overlap(&src, &target);
        let concrete_cast =
            !self.involves_type_parameter(&src) && !self.involves_type_parameter(&target);
        let ok = generic_cast
            || matches!(self.apparent_type(&src), Type::Error)
            || matches!(self.apparent_type(&target), Type::Error)
            || (concrete_cast
                && self.apparent_type(&src).is_numeric()
                && self.apparent_type(&target).is_numeric()
                && if self.apparent_type(&(src)) == Type::F16
                    || self.apparent_type(&(target)) == Type::F16
                {
                    matches!(
                        (&self.apparent_type(&src), &self.apparent_type(&target)),
                        (Type::F16, Type::F16 | Type::F32 | Type::F64)
                            | (Type::F32 | Type::F64, Type::F16)
                    )
                } else {
                    true
                })
            || (concrete_cast
                && matches!(self.apparent_type(&src), Type::Enum(_))
                && self.apparent_type(&target).is_integer())
            || (matches!(self.apparent_type(&src), Type::Object)
                && self.is_reference_class(&target))
            || (matches!(&self.apparent_type(&src), Type::Nullable(inner) if self.apparent_type(inner) == Type::Object)
                && self.is_reference_class(&target))
            || ((self.in_json_argument || self.in_for_of_subject)
                && self.apparent_type(&(target)) == Type::Object
                && self.is_reference_class(&src));
        if !ok {
            let from_n = self.type_name(&src);
            let to_n = self.type_name(&target);
            self.reject_subset(
                if src == target {
                    RejectionSite::IdentityAssertion
                } else if matches!(self.apparent_type(&target), Type::Enum(_))
                    && self.apparent_type(&src).is_integer()
                {
                    RejectionSite::IntegerEnumAssertion
                } else if matches!(
                    (&self.apparent_type(&src), &self.apparent_type(&target)),
                    (Type::Nullable(_), Type::Class(_))
                ) {
                    RejectionSite::NullableClassAssertion
                } else if self.apparent_type(&src) == Type::Str
                    && matches!(self.apparent_type(&target), Type::StringAlias(_))
                {
                    RejectionSite::StringAliasAssertion
                } else {
                    RejectionSite::InvalidAssertion
                },
                format!(
                    "`as` converts between sized numerics, enum to integer, or narrows \
                     `object | null` to a class; cannot convert `{}` to `{}`",
                    from_n, to_n
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Cast(Box::new(inner)),
            ty: target,
            pos,
        }
    }
}
