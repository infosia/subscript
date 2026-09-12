//! Checks the operator expressions, the conditional expression, `yield`, and `as`.

use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::stmt::narrow_paths;
use crate::check::{static_member_symbol, Checker, FnCtx, Local};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
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
                if operand.ty == Type::F16 {
                    self.error_diverging(
                        RuleCode::S014,
                        "arithmetic on `f16` is not supported; compute via `as f32`",
                        pos.clone(),
                        Divergence::StorageOnlyFloat16,
                    );
                    return self.err_expr(pos);
                }
                if !operand.ty.is_numeric() && !matches!(operand.ty, Type::Error) {
                    let name = self.type_name(&operand.ty);
                    self.error(
                        RuleCode::S100,
                        format!("unary `-` requires a numeric operand, got `{}`", name),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let ty = operand.ty.clone();
                hir::Expr {
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
                if !matches!(operand.ty, Type::Bool | Type::Error) {
                    let name = self.type_name(&operand.ty);
                    self.error(
                        RuleCode::S100,
                        format!("`!` requires a boolean operand, got `{}`", name),
                        pos.clone(),
                    );
                }
                hir::Expr {
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
                if !operand.ty.is_integer() && !matches!(operand.ty, Type::Error) {
                    let name = self.type_name(&operand.ty);
                    self.error(
                        RuleCode::S100,
                        format!("`~` requires an integer operand, got `{}`", name),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let ty = operand.ty.clone();
                hir::Expr {
                    kind: ExprKind::Unary {
                        op: UnOp::BitNot,
                        operand: Box::new(operand),
                    },
                    ty,
                    pos,
                }
            }
            ast::UnaryOp::Delete => {
                self.error(
                    RuleCode::S100,
                    "the `delete` operator is not in the language; use `Context.free`",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                self.error(
                    RuleCode::S100,
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
            kind: ExprKind::Local(name),
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
                    self.error_diverging(
                        RuleCode::S100,
                        format!("{spelling} cannot be used as a value"),
                        pos.clone(),
                        Divergence::ClassIndexSignature,
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
                    self.error_diverging(
                        RuleCode::S100,
                        format!("{spelling} cannot be used as a value"),
                        pos.clone(),
                        Divergence::NamedAccessor,
                    );
                    return self.err_expr(pos);
                }
                _ => {}
            }
        }
        let target_ty = place.ty().clone();
        if !target_ty.is_numeric() && !matches!(target_ty, Type::Error) {
            let name = self.type_name(&target_ty);
            self.error(
                RuleCode::S100,
                format!("`++`/`--` require a numeric target, got `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if target_ty == Type::F16 {
            self.error_diverging(
                RuleCode::S014,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
                Divergence::StorageOnlyFloat16,
            );
            return self.err_expr(pos);
        }
        let op = if u.op == ast::UpdateOp::PlusPlus {
            BinOp::Add
        } else {
            BinOp::Sub
        };
        let one = hir::Expr {
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
                    self.error(
                        RuleCode::S100,
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
                    kind: ExprKind::Call {
                        callee: Callee::Method {
                            recv: Box::new(receiver),
                            name: "set".to_string(),
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
                    self.error(
                        RuleCode::S100,
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
                    self.error(
                        RuleCode::S100,
                        format!("write accessor `{name}` has no parameter signature"),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                self.require_assignable(
                    &result.expr.ty.clone(),
                    &parameter.ty,
                    result.expr.pos.clone(),
                    "the assignment",
                );
                let callee = if let Some(receiver) = receiver {
                    Callee::Method {
                        recv: Box::new(receiver),
                        name: write_name,
                    }
                } else {
                    Callee::Func(static_member_symbol(
                        &self.classes[class.0].name,
                        &write_name,
                    ))
                };
                hir::Expr {
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
                    kind: ExprKind::Assign {
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
        if matches!(b.op, B::EqEqEq | B::NotEqEq) {
            if let Some(presence) = self.check_absence_presence_comparison(b, fx, pos.clone()) {
                return presence;
            }
        }
        match b.op {
            B::LogicalAnd | B::LogicalOr => {
                let left = self.check_expr(&b.left, None, fx);
                let right = self.check_expr(&b.right, None, fx);
                for side in [&left, &right] {
                    if !matches!(side.ty, Type::Bool | Type::Error) {
                        let name = self.type_name(&side.ty);
                        self.error(
                            RuleCode::S100,
                            format!("logical operators require booleans, got `{}`", name),
                            side.pos.clone(),
                        );
                    }
                }
                let op = if b.op == B::LogicalAnd {
                    BinOp::And
                } else {
                    BinOp::Or
                };
                hir::Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty: Type::Bool,
                    pos,
                }
            }
            B::EqEq | B::NotEq => {
                self.error(
                    RuleCode::S100,
                    "loose equality is not in the language; use `===` / `!==`",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            B::NullishCoalescing => self.check_nullish(b, fx, pos),
            B::In | B::InstanceOf | B::Exp => {
                self.error(
                    RuleCode::S100,
                    "operator outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                let arith = matches!(b.op, B::Add | B::Sub | B::Mul | B::Div | B::Mod);
                let outer: Option<Type> = if arith { ctx.cloned() } else { None };
                let literal_ctx = |t: &Type| -> Option<Type> {
                    (t.is_numeric() || matches!(t, Type::StringAlias(_))).then(|| t.clone())
                };
                let (left, right);
                if literalish(&b.left) && !literalish(&b.right) {
                    let r = self.check_expr(&b.right, outer.as_ref(), fx);
                    let c = literal_ctx(&r.ty).or(outer);
                    left = self.check_expr(&b.left, c.as_ref(), fx);
                    right = r;
                } else {
                    left = self.check_expr(&b.left, outer.as_ref(), fx);
                    let c = if literalish(&b.right) {
                        literal_ctx(&left.ty).or(outer)
                    } else {
                        outer
                    };
                    right = self.check_expr(&b.right, c.as_ref(), fx);
                }
                self.bin_result(b.op, left, right, pos, BinUse::Expression)
                    .expr
            }
        }
    }

    pub(super) fn reject_unbound_optional_chain(
        &mut self,
        chain: &ast::OptChainExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let plan = self.check_optional_plan(chain, fx);
        if plan.value.ty == Type::Error {
            return self.err_expr(pos);
        }
        let name = self.type_name(&plan.value.ty);
        self.error_diverging(
            RuleCode::S012,
            format!(
                "an optional chain has type `{name} | undefined` in TypeScript; give it a fallback with `??` or use it as a statement"
            ),
            pos.clone(),
            Divergence::OptionalChainUnbound,
        );
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
        if plan.value.ty == Type::Error {
            out.push(hir::Stmt::Expr(self.err_expr(pos)));
            return out;
        }
        if !plan.ends_in_call {
            let name = self.type_name(&plan.value.ty);
            self.error_diverging(
                RuleCode::S012,
                format!(
                    "an optional chain has type `{name} | undefined` in TypeScript; give it a fallback with `??` or use it as a statement"
                ),
                pos.clone(),
                Divergence::OptionalChainUnbound,
            );
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

        if current.ty == Type::Error {
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
                        self.error_diverging(
                            RuleCode::S100,
                            "an optional chain cannot use `?.[i]`; narrow the receiver and use `[i]`",
                            self.pos(member.span),
                            Divergence::OptionalChainIndex,
                        );
                        return OptionalPlan {
                            tests,
                            value: self.err_expr(self.pos(member.span)),
                            ends_in_call: false,
                        };
                    }
                    if *tested {
                        let Some(inner) = self.require_nullable_operand(
                            &current,
                            "the tested receiver",
                            Divergence::OptionalChainNonNullable,
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
                            self.error(
                                RuleCode::S100,
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
                            None,
                            fx,
                            self.pos(call.span),
                        );
                        index += 2;
                        ends_in_call = true;
                        continue;
                    }

                    current = self.check_optional_member(current, member, fx);
                    index += 1;
                    ends_in_call = false;
                }
                OptionalStep::Call { call, tested } => {
                    if *tested {
                        self.error(
                            RuleCode::S100,
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
                false,
            ),
            ast::MemberProp::Computed(property) => {
                let index_context = match &receiver.ty {
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
                self.error(
                    RuleCode::S100,
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

        let left = self.check_expr(&binary.left, None, fx);
        if left.ty == Type::Error {
            return self.err_expr(pos);
        }
        let Some(inner) = self.require_nullable_operand(
            &left,
            "the left operand of `??`",
            Divergence::NullishNonNullable,
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
        if plan.value.ty == Type::Error {
            return self.err_expr(pos);
        }
        let value_ty = plan.value.ty.clone();
        match value_ty {
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
        let nullable = Type::Nullable(Box::new(inner.clone()));
        if right.ty == nullable {
            return nullable;
        }
        if right.ty != Type::Error {
            self.require_assignable(&right.ty.clone(), inner, right.pos.clone(), what);
        }
        Type::Error
    }

    fn require_nullable_operand(
        &mut self,
        operand: &hir::Expr,
        what: &str,
        divergence: Divergence,
    ) -> Option<Type> {
        if let Type::Nullable(inner) = &operand.ty {
            return Some((**inner).clone());
        }
        let name = self.type_name(&operand.ty);
        self.error_diverging(
            RuleCode::S100,
            format!("{what} has type `{name}`, which is not nullable"),
            operand.pos.clone(),
            divergence,
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
            Local {
                ty: nullable.clone(),
                mutable: true,
                holds_capturing: false,
                async_origins: HashSet::new(),
            },
        );
        fx.push_synthetic_prefix(hir::Stmt::Let {
            name: name.clone(),
            ty: nullable.clone(),
            mutable: true,
            dispose: false,
            init: hir::Expr {
                kind: ExprKind::Null,
                ty: Type::Null,
                pos: pos.clone(),
            },
            pos: pos.clone(),
        });
        let target = hir::Expr {
            kind: ExprKind::Local(name.clone()),
            ty: nullable.clone(),
            pos: pos.clone(),
        };
        let assigned = hir::Expr {
            kind: ExprKind::Assign {
                op: None,
                target: Box::new(target),
                value: Box::new(operand),
            },
            ty: nullable,
            pos: pos.clone(),
        };
        let value = hir::Expr {
            kind: ExprKind::Local(name),
            ty: inner,
            pos: pos.clone(),
        };
        (self.null_test(assigned, pos), value)
    }

    fn null_test(&self, value: hir::Expr, pos: Pos) -> hir::Expr {
        hir::Expr {
            kind: ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(value),
                right: Box::new(hir::Expr {
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
            self.error(
                RuleCode::S012,
                "`undefined` is legal only in a presence test on an absence-capable descriptor member",
                self.pos(undefined_source.span()),
            );
            return Some(self.err_expr(pos));
        }

        let member_source = if left_undefined {
            &*binary.right
        } else {
            &*binary.left
        };
        let checked = match unparen_expr(member_source) {
            ast::Expr::Member(member) => self.check_member_read_inner(member, fx, true),
            other => self.check_expr(other, None, fx),
        };
        if !self.is_absence_capable_member_expr(&checked) {
            self.error(
                RuleCode::S012,
                "`undefined` is legal only in a presence test on an absence-capable descriptor member",
                self.pos(undefined_source.span()),
            );
            return Some(self.err_expr(pos));
        }

        Some(hir::Expr {
            kind: ExprKind::AbsenceTest {
                value: Box::new(checked),
                negated: binary.op == ast::BinaryOp::NotEqEq,
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
        let lt = left.ty.clone();
        let rt = right.ty.clone();
        let operand_error = matches!(lt, Type::Error) || matches!(rt, Type::Error);
        let suppress_error = match use_kind {
            BinUse::Expression => operand_error,
            BinUse::CompoundAssignment => matches!(lt, Type::Error),
        };
        let mixed_numeric = lt.is_numeric() && rt.is_numeric() && lt != rt;
        let arithmetic = matches!(op, B::Add | B::Sub | B::Mul | B::Div | B::Mod);
        let f16_arithmetic = arithmetic
            && match use_kind {
                BinUse::Expression => lt == Type::F16 || rt == Type::F16,
                BinUse::CompoundAssignment => lt == Type::F16,
            };
        if f16_arithmetic {
            self.error_diverging(
                RuleCode::S014,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
                Divergence::StorageOnlyFloat16,
            );
            return BinResult {
                expr: self.err_expr(pos),
                terminal: true,
            };
        }
        let mk = |op: BinOp, ty: Type| hir::Expr {
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
                if use_kind == BinUse::CompoundAssignment && (lt.is_numeric() || lt == Type::Str) {
                    (BinOp::Add, lt.clone(), true)
                } else if lt == Type::Str && rt == Type::Str {
                    (BinOp::Add, Type::Str, true)
                } else if lt.is_numeric() && lt == rt {
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
                if lt.is_numeric() && (use_kind == BinUse::CompoundAssignment || lt == rt) {
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
                let comparable =
                    (lt.is_numeric() && lt == rt) || (matches!(lt, Type::Enum(_)) && lt == rt);
                (hop, Type::Bool, comparable)
            }
            B::EqEqEq | B::NotEqEq => {
                let hop = if op == B::EqEqEq {
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
                (hop, Type::Bool, null_cmp || same_scalar)
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
                if lt.is_integer() && lt == rt {
                    (hop, lt.clone(), true)
                } else if use_kind == BinUse::CompoundAssignment && lt.is_integer() {
                    (hop, lt.clone(), true)
                } else if lt.is_integer() && rt.is_integer() {
                    // Q18: mixed-width bitwise requires `as`.
                    (hop, Type::Error, suppress_error)
                } else {
                    (hop, Type::Error, suppress_error)
                }
            }
            _ => (BinOp::Add, Type::Error, suppress_error),
        };
        if ok || suppress_error {
            return BinResult {
                expr: mk(hop, if operand_error { Type::Error } else { ty }),
                terminal: false,
            };
        }
        if use_kind == BinUse::CompoundAssignment {
            let name = self.type_name(&lt);
            self.error(
                RuleCode::S100,
                format!("compound assignment is not defined for `{}`", name),
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
                B::EqEqEq | B::NotEqEq | B::Lt | B::LtEq | B::Gt | B::GtEq
            )
        {
            self.reject_api_form(
                "Date",
                "direct comparison",
                "Date direct comparison",
                pos.clone(),
            );
            return BinResult {
                expr: self.err_expr(pos),
                terminal: false,
            };
        }
        let ln = self.type_name(&lt);
        let rn = self.type_name(&rt);
        if mixed_numeric {
            let family = if matches!(
                op,
                B::BitAnd | B::BitOr | B::BitXor | B::LShift | B::RShift | B::ZeroFillRShift
            ) {
                "bitwise"
            } else {
                "arithmetic"
            };
            self.error_diverging(
                RuleCode::S007,
                format!(
                    "mixed-type {} (`{}` and `{}`) requires an explicit `as` conversion",
                    family, ln, rn
                ),
                pos.clone(),
                Divergence::SizedOperandWidths,
            );
        } else {
            self.error(
                RuleCode::S100,
                format!("operator not defined for `{}` and `{}`", ln, rn),
                pos.clone(),
            );
        }
        BinResult {
            expr: self.err_expr(pos),
            terminal: false,
        }
    }

    pub(super) fn check_cond(
        &mut self,
        c: &ast::CondExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let cond = self.check_expr(&c.test, None, fx);
        if !matches!(cond.ty, Type::Bool | Type::Error) {
            let name = self.type_name(&cond.ty);
            self.error(
                RuleCode::S100,
                format!("condition must be boolean, got `{}`", name),
                cond.pos.clone(),
            );
        }
        let (then_extra, else_extra) = narrow_paths(&cond);
        let mut base = fx.narrowed.clone();

        fx.narrowed = base.iter().cloned().chain(then_extra.clone()).collect();
        let then = self.check_expr(&c.cons, ctx, fx);
        // Keep kills: facts removed inside the arm stay removed.
        base.retain(|key| fx.narrowed.contains(key) || then_extra.contains(key));

        fx.narrowed = base.iter().cloned().chain(else_extra.clone()).collect();
        let els = self.check_expr(&c.alt, ctx, fx);
        base.retain(|key| fx.narrowed.contains(key) || else_extra.contains(key));
        fx.narrowed = base;

        let ty = if let Some(context) = ctx {
            self.require_assignable(
                &then.ty.clone(),
                context,
                then.pos.clone(),
                "the then branch",
            );
            self.require_assignable(&els.ty.clone(), context, els.pos.clone(), "the else branch");
            context.clone()
        } else {
            let then_ty = then.ty.clone();
            self.require_assignable_with(
                &els.ty.clone(),
                &then_ty,
                els.pos.clone(),
                "the else branch",
                Some(Divergence::ConditionalWithoutContext),
            );
            then_ty
        };
        hir::Expr {
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
            self.error(
                RuleCode::S100,
                "`yield` is only available inside a `function*` coroutine",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if y.delegate {
            self.error(
                RuleCode::S100,
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
                if known_yield.is_none() {
                    if let Some(frame) = fx.frames.last_mut() {
                        frame.yield_ty = Some(Type::Void);
                    }
                }
                None
            }
        };
        hir::Expr {
            kind: ExprKind::Yield(arg),
            ty: Type::Void,
            pos,
        }
    }

    pub(super) fn check_as(&mut self, a: &ast::TsAsExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let target = self.resolve_type(&a.type_ann);
        let inner = self.check_expr(&a.expr, None, fx);
        let src = inner.ty.clone();
        let ok = matches!(src, Type::Error)
            || matches!(target, Type::Error)
            || (src.is_numeric()
                && target.is_numeric()
                && if src == Type::F16 || target == Type::F16 {
                    matches!(
                        (&src, &target),
                        (Type::F16, Type::F16 | Type::F32 | Type::F64)
                            | (Type::F32 | Type::F64, Type::F16)
                    )
                } else {
                    true
                })
            || (matches!(src, Type::Enum(_)) && target.is_integer())
            || (matches!(src, Type::Object) && self.is_reference_class(&target))
            || (matches!(&src, Type::Nullable(inner) if **inner == Type::Object)
                && self.is_reference_class(&target))
            || ((self.in_json_argument || self.in_for_of_subject)
                && target == Type::Object
                && self.is_reference_class(&src));
        if !ok {
            let from_n = self.type_name(&src);
            let to_n = self.type_name(&target);
            self.error(
                RuleCode::S100,
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
            kind: ExprKind::Cast(Box::new(inner)),
            ty: target,
            pos,
        }
    }
}
