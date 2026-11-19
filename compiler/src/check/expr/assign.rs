//! Checks assignment expressions and the places that they write.

use crate::check::rejection::RejectionSite;
use oxc_ast::ast;
use oxc_span::GetSpan;

use crate::check::{static_member_symbol, Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, Callee, ExprKind};
use crate::types::Type;

use super::{
    assign_binary_op, assign_op, path_key, write_spelling, BinUse, Place, PlaceSource,
    WriteExpression,
};

impl<'p> Checker<'p> {
    pub(super) fn check_assign(
        &mut self,
        a: &ast::AssignmentExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
        statement_position: bool,
        mut prefix: Option<&mut crate::check::SyntheticPrefix>,
    ) -> hir::Expr {
        use ast::AssignmentOperator as A;
        if let Some(logical) = super::logical_assign::LogicalAssign::of(a.operator) {
            return self.check_logical_assign_value(a, logical, fx, pos);
        }
        let mut length_place = None;
        if let Some(member) = a.left.as_member_expression() {
            let view = super::member_view(member);
            let member = &view;
            if let Some(property_span) = match &member.prop {
                super::MemberKey::Ident(name) if name.name == "length" => Some(name.span),
                _ => None,
            } {
                if self.reject_static_this_member(member, fx) {
                    return self.err_expr(pos);
                }
                if let Some(place) =
                    self.check_namespace_place(member.object, "length", self.pos(property_span), fx)
                {
                    length_place = Some(place);
                } else {
                    let receiver = self.check_write_receiver(member, fx);
                    if matches!(self.apparent_type(&receiver.ty), Type::Array(_)) {
                        if statement_position
                            && a.operator == A::Assign
                            && matches!(&a.right, ast::Expression::NumericLiteral(value) if value.value == 0.0 && value.raw.as_ref().is_some_and(|raw| raw == "0"))
                        {
                            return hir::Expr {
                                pending_work: None,
                                kind: ExprKind::Call {
                                    callee: Callee::Method {
                                        recv: Box::new(receiver),
                                        name: hir::Symbol::from_full_text("[[array_clear]]"),
                                    },
                                    args: Vec::new(),
                                },
                                ty: Type::Void,
                                pos,
                            };
                        }
                        self.reject_subset(
                            RejectionSite::ArrayLengthStore,
                            "only `xs.length = 0` as a statement is accepted; use `splice` or `pop` to remove elements",
                            self.pos(property_span),
                        );
                        return self.err_expr(pos);
                    }
                    length_place = Some(self.check_named_member_place(
                        member,
                        receiver,
                        "length",
                        self.pos(property_span),
                        fx,
                    ));
                }
            }
        }
        let operation = assign_op(a.operator);
        let op = if a.operator == A::Assign {
            None
        } else if let Some((op, _)) = operation {
            Some(op)
        } else {
            if self.reject_readonly_assignment_target(&a.left, fx) {
                return self.err_expr(pos);
            }
            self.reject_subset(
                RejectionSite::PowerAssignment,
                "assignment operator outside the decided surface",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        // §107.3: a pattern binds new names. A pattern that writes
        // existing targets carries its own reason.
        if matches!(
            a.left,
            ast::AssignmentTarget::ArrayAssignmentTarget(_)
                | ast::AssignmentTarget::ObjectAssignmentTarget(_)
        ) {
            let target = &a.left;
            if self.reject_readonly_assignment_target(&a.left, fx) {
                return self.err_expr(pos);
            }
            self.reject_subset(RejectionSite::DestructuringAssignment, "a destructuring assignment needs an evaluation and write order for its targets; a binding pattern declares its names", self.pos(target.span()));
            return self.err_expr(pos);
        }
        let source = match &a.left {
            ast::AssignmentTarget::AssignmentTargetIdentifier(binding) => {
                PlaceSource::Ident(binding)
            }
            target if target.as_member_expression().is_some() => {
                match target.as_member_expression() {
                    Some(member) => PlaceSource::Member(member),
                    None => PlaceSource::Unsupported,
                }
            }
            _ => PlaceSource::Unsupported,
        };
        let place = length_place
            .unwrap_or_else(|| self.check_assign_target(source, op.is_some(), fx, &pos));
        #[cfg(test)]
        if self.narrowing_analysis.is_some() {
            place.record_kind();
        }
        let target_ty = place.ty().clone();
        if self.apparent_type(&target_ty) == Type::TaskGroup {
            self.reject_subset(
                RejectionSite::TaskGroupPosition,
                "a TaskGroup cannot be assigned",
                pos.clone(),
            );
        }
        let value_ctx = if matches!(self.apparent_type(&target_ty), Type::Error) {
            None
        } else {
            Some(target_ty.clone())
        };
        if op.is_some() {
            self.mark_operand_calls(&a.right);
        }
        let mut value = self.check_expr(&a.right, value_ctx.as_ref(), fx);
        let signature_write = match &place {
            Place::IndexSignature {
                receiver,
                index,
                signature,
                pos: target_pos,
            } => Some((
                receiver.clone(),
                index.clone(),
                signature.clone(),
                target_pos.clone(),
            )),
            _ => None,
        };
        if let Some((receiver, index, signature, target_pos)) = signature_write {
            if !statement_position {
                let spelling = write_spelling(WriteExpression::Assign(a), "a[i]");
                self.reject_subset(
                    RejectionSite::IndexAssignmentExpressionValue,
                    format!("{spelling} cannot be used as a value"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            if signature.readonly {
                self.reject_subset(
                    RejectionSite::ReadonlyIndexAssignment,
                    "`a[i] = v` cannot write through a readonly index signature",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let (receiver, index, mut value) = if let Some((bin, _)) = operation {
                let prefix = prefix
                    .as_deref_mut()
                    .expect("a statement assignment must provide compound-write storage");
                let receiver = self.stabilize_compound_operand(receiver, "receiver", prefix);
                let index = self.stabilize_compound_operand(index, "index", prefix);
                let read = Place::IndexSignature {
                    receiver: receiver.clone(),
                    index: index.clone(),
                    signature: signature.clone(),
                    pos: target_pos,
                }
                .into_read(self);
                let Some(binary_op) = assign_binary_op(bin) else {
                    return self.err_expr(pos);
                };
                let result = self.bin_result(
                    binary_op,
                    read,
                    value,
                    pos.clone(),
                    BinUse::CompoundAssignment,
                );
                if result.terminal {
                    return result.expr;
                }
                (receiver, index, result.expr)
            } else {
                (receiver, index, value)
            };
            self.require_expr_assignable(&mut value, &signature.element_ty, fx, "the assignment");
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(receiver),
                        name: hir::Symbol::from_full_text("set"),
                    },
                    args: vec![index, value],
                },
                ty: Type::Void,
                pos,
            };
        }
        let static_accessor_write = match &place {
            Place::Accessor {
                class,
                receiver: None,
                name,
                ..
            } => Some((*class, name.clone())),
            _ => None,
        };
        if let Some((id, name)) = static_accessor_write {
            let class_name = self.classes[id.0].name.clone();
            if !statement_position {
                let target = format!("{class_name}.{name}");
                let spelling = write_spelling(WriteExpression::Assign(a), &target);
                self.reject_subset(
                    RejectionSite::StaticAccessorAssignmentExpressionValue,
                    format!("{spelling} cannot be used as a value"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let write_name = format!("{name}=");
            let Some(signature) = self.class_sigs[id.0]
                .static_methods
                .get(&write_name)
                .cloned()
            else {
                let target = format!("{class_name}.{name}");
                let spelling = write_spelling(WriteExpression::Assign(a), &target);
                self.reject_subset(
                    RejectionSite::ReadonlyStaticAccessorAssignment,
                    format!("{spelling} cannot write through a read-only accessor"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let signature = self.member_signature(id, signature, fx);
            let Some(parameter) = signature.params.first() else {
                self.reject_subset(
                    RejectionSite::StaticSetterParameterMissing,
                    format!(
                        "static write accessor `{class_name}.{name}` has no parameter signature"
                    ),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let mut value = if let Some((bin, _)) = operation {
                let read = place.into_read(self);
                let Some(binary_op) = assign_binary_op(bin) else {
                    return self.err_expr(pos);
                };
                let result = self.bin_result(
                    binary_op,
                    read,
                    value,
                    pos.clone(),
                    BinUse::CompoundAssignment,
                );
                if result.terminal {
                    return result.expr;
                }
                result.expr
            } else {
                value
            };
            self.require_expr_assignable(&mut value, parameter.ty(), fx, "the assignment");
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: Callee::Func(hir::Symbol::from_full_text(static_member_symbol(
                        id,
                        &class_name,
                        &write_name,
                    ))),
                    args: vec![value],
                },
                ty: Type::Void,
                pos,
            };
        }
        let accessor_write = match &place {
            Place::Accessor {
                class,
                receiver: Some(receiver),
                name,
                ..
            } => Some((*class, receiver.clone(), name.clone())),
            _ => None,
        };
        if let Some((id, recv, name)) = accessor_write {
            if !statement_position {
                let target = format!("x.{name}");
                let spelling = write_spelling(WriteExpression::Assign(a), &target);
                self.reject_subset(
                    RejectionSite::AccessorAssignmentExpressionValue,
                    format!("{spelling} cannot be used as a value"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let write_name = format!("{name}=");
            let Some(signature) = self.class_sigs[id.0].methods.get(&write_name).cloned() else {
                let target = format!("x.{name}");
                let spelling = write_spelling(WriteExpression::Assign(a), &target);
                self.reject_subset(
                    RejectionSite::ReadonlyAccessorAssignment,
                    format!("{spelling} cannot write through a read-only accessor"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let signature = self.member_signature(id, signature, fx);
            let Some(parameter) = signature.params.first() else {
                self.reject_subset(
                    RejectionSite::SetterParameterMissing,
                    format!("write accessor `{name}` has no parameter signature"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let (recv, mut value) = if let Some((bin, _)) = operation {
                let prefix =
                    prefix.expect("a statement assignment must provide compound-write storage");
                let recv = self.stabilize_compound_operand(recv, "receiver", prefix);
                let read = Place::Accessor {
                    class: id,
                    receiver: Some(recv.clone()),
                    name: name.clone(),
                    ty: target_ty.clone(),
                    pos: pos.clone(),
                }
                .into_read(self);
                let Some(binary_op) = assign_binary_op(bin) else {
                    return self.err_expr(pos);
                };
                let result = self.bin_result(
                    binary_op,
                    read,
                    value,
                    pos.clone(),
                    BinUse::CompoundAssignment,
                );
                if result.terminal {
                    return result.expr;
                }
                (recv, result.expr)
            } else {
                (recv, value)
            };
            self.require_expr_assignable(&mut value, parameter.ty(), fx, "the assignment");
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(recv),
                        name: hir::Symbol::from_full_text(write_name),
                    },
                    args: vec![value],
                },
                ty: Type::Void,
                pos,
            };
        }
        let target = place.into_read(self);
        // §316: an inferred readonly field keeps its initializer literal.
        if matches!(target.kind, ExprKind::Field { .. }) {
            if let Some(expected) = self.checked_string_literal_operand(&a.right, &target, fx) {
                let actual = self.checked_string_literal_operand(&a.right, &value, fx);
                if op.is_some() || actual.as_ref() != Some(&expected) {
                    self.reject_subset(
                        RejectionSite::ReadonlyLiteralStore,
                        format!("readonly field assignment expects literal {expected:?}"),
                        value.pos.clone(),
                    );
                    return self.err_expr(pos);
                }
            }
        }
        if let ExprKind::Field { obj, name } = &target.kind {
            if let Type::Class(class) = self.apparent_type(&obj.ty) {
                if let Some(field) = self.class_sigs[class.0].fields.get(name) {
                    if !field.mutable && !field.annotated {
                        if let Some((expected, spelling)) =
                            field.initializer.as_ref().and_then(|init| {
                                Some((
                                    numeric_or_boolean_literal(&init.expression)?,
                                    numeric_or_boolean_literal_spelling(&init.expression)?,
                                ))
                            })
                        {
                            if op.is_some()
                                || numeric_or_boolean_literal(&a.right).as_ref() != Some(&expected)
                            {
                                self.reject_subset(
                                    RejectionSite::ReadonlyLiteralStore,
                                    format!("readonly field assignment expects literal {spelling}"),
                                    value.pos.clone(),
                                );
                                return self.err_expr(pos);
                            }
                        }
                    }
                }
            }
        }
        let result_ty = if let Some(bin) = op {
            let Some(binary_op) = assign_binary_op(bin) else {
                return self.err_expr(pos);
            };
            let result = self.bin_result(
                binary_op,
                target.clone(),
                value.clone(),
                pos.clone(),
                BinUse::CompoundAssignment,
            );
            if result.terminal {
                return result.expr;
            }
            if let ExprKind::Binary { right, .. } = result.expr.kind {
                value = *right;
            }
            result.expr.ty
        } else {
            value.ty.clone()
        };
        if op.is_none() {
            self.check_function_binding_assignment(&target, &a.right, &value, fx);
            self.require_expr_assignable(&mut value, &target_ty, fx, "the assignment");
        } else {
            self.require_assignable(
                &if op.is_some() {
                    result_ty.clone()
                } else {
                    value.ty.clone()
                },
                &target_ty,
                value.pos.clone(),
                "the assignment",
            );
        }
        if op.is_none() && self.apparent_type(&target_ty).holds_async_handle() {
            let origins = self.expr_async_origins(&value, fx);
            // §188.1 rule 3: a local takes the origins; an element of a
            // local array adds them; a field or a module global discharges them.
            match &target.kind {
                ExprKind::Local(name, _, _) => fx.set_local_async_origins(name, origins),
                _ => match local_element_root(&target) {
                    Some(name) => {
                        let mut stored = fx.local_async_origins(name);
                        stored.extend(origins);
                        fx.set_local_async_origins(name, stored);
                    }
                    None => fx.handle_async_origins(&origins),
                },
            }
        }
        if let Some(key) = path_key(&target) {
            let prefix = format!("{key}.");
            fx.ended_shared_narrowing
                .retain(|path| path != &key && !path.starts_with(&prefix));
            fx.narrowed
                .retain(|path| path != &key && !path.starts_with(&prefix));
        }
        let assigned = hir::Expr {
            pending_work: None,
            kind: ExprKind::Assign {
                update: None,
                op,
                target: Box::new(target),
                value: Box::new(value),
            },
            ty: result_ty,
            pos,
        };
        self.end_shared_narrowing(&assigned, fx);
        assigned
    }

    /// `arithmetic` is true for the target of a compound assignment, an
    /// increment, or a decrement (§209.1 rule 3).
    pub(super) fn check_assign_target(
        &mut self,
        target: PlaceSource<'_>,
        arithmetic: bool,
        fx: &mut FnCtx<'p>,
        pos: &Pos,
    ) -> Place {
        let previous = fx
            .field_initializer
            .as_ref()
            .is_some_and(|context| context.write);
        let writes_this = match &target {
            PlaceSource::Member(member) => {
                let mut object = super::unparen_expr(member.object());
                loop {
                    object = match object {
                        expression if expression.as_member_expression().is_some() => expression
                            .as_member_expression()
                            .map_or(expression, |member| super::unparen_expr(member.object())),
                        ast::Expression::TSAsExpression(cast) => {
                            super::unparen_expr(&cast.expression)
                        }
                        _ => break,
                    };
                }
                matches!(object, ast::Expression::ThisExpression(_))
            }
            _ => false,
        };
        if let Some(context) = &mut fx.field_initializer {
            context.write = writes_this;
        }
        let previous_arithmetic = std::mem::replace(&mut fx.arithmetic_write, arithmetic);
        let place = self.check_assign_target_inner(target, fx, pos);
        if let Place::Field(path) | Place::Index(path) = &place {
            self.reject_write_through_value_temporary(path);
        }
        fx.arithmetic_write = previous_arithmetic;
        if let Some(context) = &mut fx.field_initializer {
            context.write = previous;
        }
        place
    }

    /// §218.1 rule 12a: a path that stays in value storage and reaches a value
    /// temporary writes a copy. A reference class or a dynamic array on the
    /// path ends the walk, because the write reaches shared storage.
    fn reject_write_through_value_temporary(&mut self, path: &hir::Expr) {
        let mut step = path;
        loop {
            let (ExprKind::Field { obj, .. } | ExprKind::Index { obj, .. }) = &step.kind else {
                return;
            };
            let value_storage = self.is_value_class(&obj.ty)
                || matches!(
                    self.apparent_type(&obj.ty),
                    Type::FixedArray(..) | Type::IterResult(_)
                );
            if !value_storage {
                return;
            }
            let mut root = obj.as_ref();
            while let ExprKind::Cast(inner) = &root.kind {
                root = inner;
            }
            match &root.kind {
                ExprKind::Field { .. } | ExprKind::Index { .. } => step = root,
                ExprKind::Local(..)
                | ExprKind::Global(_, _)
                | ExprKind::This
                | ExprKind::Seq { .. } => return,
                _ => {
                    let result = self.type_name(&root.ty);
                    self.reject_subset(
                        RejectionSite::WriteThroughValueTemporary,
                        format!(
                            "this write changes a copy of the `{result}` result, and nothing \
                             reads that copy; bind the result to a variable, or assign to the \
                             source"
                        ),
                        path.pos.clone(),
                    );
                    return;
                }
            }
        }
    }

    fn check_assign_target_inner(
        &mut self,
        target: PlaceSource<'_>,
        fx: &mut FnCtx<'p>,
        pos: &Pos,
    ) -> Place {
        match target {
            PlaceSource::Ident(ident) => {
                let name = ident.name.to_string();
                let ident_pos = self.pos(ident.span);
                if let Some(local) = self.lookup_local_for_write(&name, &ident_pos, fx) {
                    if !local.mutable {
                        // §143 rule 2: the opaque check reports this diagnostic.
                        self.reject_subset(
                            RejectionSite::ConstLocalAssignment,
                            format!("cannot rebind `const` binding `{}`", name),
                            ident_pos.clone(),
                        );
                    }
                    return Place::Local(hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Local(name, local.ty.clone(), local.annotated),
                        ty: local.ty,
                        pos: ident_pos,
                    });
                }
                let item = self.scope_item(&name, &ident_pos, fx);
                if matches!(item, Some(ScopeItem::Poisoned)) {
                    return Place::Local(self.err_expr(ident_pos));
                }
                if self
                    .scope_binding(&name)
                    .is_some_and(|binding| binding.imported)
                {
                    // §143 rule 2: the opaque check reports this diagnostic.
                    self.reject_subset(
                        RejectionSite::ImportedBindingAssignment,
                        format!("cannot assign to `{name}` because it is an import"),
                        ident_pos.clone(),
                    );
                    return Place::Local(self.err_expr(ident_pos));
                }
                if let Some(ScopeItem::Global(g)) = item {
                    let sig = self.global_sigs.get(&g).cloned();
                    if let Some(sig) = sig {
                        if !sig.mutable {
                            // §143 rule 2: the opaque check reports this diagnostic.
                            self.reject_subset(
                                RejectionSite::ConstGlobalAssignment,
                                format!("cannot rebind `const` binding `{}`", name),
                                ident_pos.clone(),
                            );
                        }
                        let ty = self.decide_global(&g, ident_pos.clone());
                        return Place::Global(hir::Expr {
                            pending_work: None,
                            kind: ExprKind::Global(hir::Symbol::from_full_text(g), true),
                            ty,
                            pos: ident_pos,
                        });
                    }
                }
                self.reject_subset(
                    RejectionSite::NonVariableAssignmentTarget,
                    format!("`{}` is not an assignable binding", name),
                    ident_pos.clone(),
                );
                Place::Local(self.err_expr(ident_pos))
            }
            PlaceSource::Member(member) => self.check_member_place(member, fx),
            PlaceSource::Unsupported => {
                self.reject_subset(
                    RejectionSite::NonPlaceAssignmentTarget,
                    "assignment target outside the decided surface",
                    pos.clone(),
                );
                Place::Local(self.err_expr(pos.clone()))
            }
        }
    }

    fn check_write_receiver(
        &mut self,
        member: &ast::MemberExpression,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        let view = super::member_view(member);
        let member = &view;
        // Resolve a readonly target through the enclosing method receiver.
        // The modifier check retains the arrow frame and rejects the write.
        let enclosing_this = if matches!(
            super::unparen_expr(member.object),
            ast::Expression::ThisExpression(_)
        ) && fx
            .frames
            .last()
            .is_some_and(|frame| frame.is_lambda && frame.this_ty.is_none())
        {
            fx.frames.iter().rev().find_map(|frame| frame.this_ty.clone()).filter(|ty| {
                matches!((self.apparent_type(ty), &member.prop), (Type::Class(class), super::MemberKey::Ident(name))
                    if self.is_readonly_field(class, name.name.as_ref()))
            })
        } else {
            None
        };
        if let Some(ty) = enclosing_this {
            if let Some(frame) = fx.frames.last_mut() {
                frame.this_ty = Some(ty);
            }
            let receiver = self.check_receiver(member.object, fx);
            if let Some(frame) = fx.frames.last_mut() {
                frame.this_ty = None;
            }
            receiver
        } else {
            self.check_receiver(member.object, fx)
        }
    }

    fn reject_readonly_assignment_target(
        &mut self,
        target: &ast::AssignmentTarget,
        fx: &mut FnCtx<'p>,
    ) -> bool {
        fn collect<'s, 'a>(
            target: &'s ast::AssignmentTarget<'a>,
            members: &mut Vec<&'s ast::MemberExpression<'a>>,
        ) {
            if let Some(member) = target.as_member_expression() {
                members.push(member);
                return;
            }
            match target {
                ast::AssignmentTarget::ArrayAssignmentTarget(array) => {
                    for element in array.elements.iter().flatten() {
                        collect_default(element, members);
                    }
                    if let Some(rest) = &array.rest {
                        collect(&rest.target, members);
                    }
                }
                ast::AssignmentTarget::ObjectAssignmentTarget(object) => {
                    for property in &object.properties {
                        if let ast::AssignmentTargetProperty::AssignmentTargetPropertyProperty(
                            property,
                        ) = property
                        {
                            collect_default(&property.binding, members);
                        }
                    }
                    if let Some(rest) = &object.rest {
                        collect(&rest.target, members);
                    }
                }
                _ => {}
            }
        }
        fn collect_default<'s, 'a>(
            target: &'s ast::AssignmentTargetMaybeDefault<'a>,
            members: &mut Vec<&'s ast::MemberExpression<'a>>,
        ) {
            if let ast::AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(default) = target
            {
                collect(&default.binding, members);
            } else if let Some(target) = target.as_assignment_target() {
                collect(target, members);
            }
        }
        let mut members = Vec::new();
        collect(target, &mut members);
        for member in members {
            let view = super::member_view(member);
            let member = &view;
            let super::MemberKey::Ident(name) = &member.prop else {
                continue;
            };
            let receiver = self.check_write_receiver(member, fx);
            if let Type::Class(class) = self.apparent_type(&receiver.ty) {
                if self.reject_readonly_field_write(
                    class,
                    name.name.as_ref(),
                    matches!(member.object, ast::Expression::ThisExpression(_)),
                    fx,
                    self.pos(name.span),
                ) {
                    return true;
                }
            }
        }
        false
    }

    fn check_member_place(&mut self, m: &ast::MemberExpression, fx: &mut FnCtx<'p>) -> Place {
        let view = super::member_view(m);
        let m = &view;
        let pos = self.pos(m.span);
        if self.reject_static_this_member(m, fx) {
            return Place::Field(self.err_expr(pos));
        }
        match &m.prop {
            super::MemberKey::Computed(c) => {
                let obj = self.check_write_receiver(m, fx);
                let index_context = match &self.apparent_type(&obj.ty) {
                    Type::Class(id) => self.classes[id.0]
                        .index_signature
                        .as_ref()
                        .map(|signature| signature.index_ty.clone())
                        .unwrap_or(Type::I32),
                    _ => Type::I32,
                };
                // The index is a read even when the receiver is a write target.
                let previous = fx
                    .field_initializer
                    .as_ref()
                    .is_some_and(|context| context.write);
                if let Some(context) = &mut fx.field_initializer {
                    context.write = false;
                }
                self.mark_operand_calls(&c.expression);
                let mut index = self.check_expr(&c.expression, Some(&index_context), fx);
                if let Some(context) = &mut fx.field_initializer {
                    context.write = previous;
                }
                if let Type::Class(id) = &self.apparent_type(&obj.ty) {
                    if let Some(signature) = self.classes[id.0].index_signature.clone() {
                        self.require_expr_assignable(
                            &mut index,
                            &signature.index_ty,
                            fx,
                            "the index",
                        );
                        return Place::IndexSignature {
                            receiver: obj,
                            index,
                            signature,
                            pos,
                        };
                    }
                }
                Place::Index(self.check_index(obj, index, pos, fx))
            }
            super::MemberKey::Ident(prop) => {
                let name = prop.name.to_string();
                let prop_pos = self.pos(prop.span);
                if let Some(place) =
                    self.check_namespace_place(m.object, &name, prop_pos.clone(), fx)
                {
                    return place;
                }
                let obj = self.check_write_receiver(m, fx);
                self.check_named_member_place(m, obj, &name, prop_pos, fx)
            }
            super::MemberKey::PrivateName(prop) => {
                self.reject_unlowered_private_name(prop);
                Place::Field(self.err_expr(pos))
            }
        }
    }

    fn check_named_member_place(
        &mut self,
        m: &ast::MemberExpression,
        obj: hir::Expr,
        name: &str,
        prop_pos: Pos,
        fx: &mut FnCtx<'p>,
    ) -> Place {
        let view = super::member_view(m);
        let m = &view;
        if let Type::Class(id) = &self.apparent_type(&obj.ty) {
            if self.classes[id.0]
                .fields
                .iter()
                .any(|field| field.name == name)
            {
                return Place::Field(self.member_on(
                    obj,
                    name,
                    prop_pos,
                    Some(matches!(m.object, ast::Expression::ThisExpression(_))),
                    fx,
                ));
            }
            if self.class_sigs[id.0].has_accessor(name) {
                if self.reject_member_access(*id, name, false, true, fx, prop_pos.clone()) {
                    return Place::Field(self.err_expr(prop_pos));
                }
                let Some(signature) = self.class_sigs[id.0].methods.get(name) else {
                    self.reject_subset(
                        RejectionSite::WriteSetterOnlyAccessor,
                        format!("read accessor `{name}` has no checker signature"),
                        prop_pos.clone(),
                    );
                    return Place::Accessor {
                        class: *id,
                        receiver: Some(obj),
                        name: name.to_owned(),
                        ty: Type::Error,
                        pos: prop_pos,
                    };
                };
                let ty = self.member_value_type(*id, &signature.ret, fx);
                let call_pos = obj.pos.clone();
                return Place::Accessor {
                    class: *id,
                    receiver: Some(obj),
                    name: name.to_owned(),
                    ty,
                    pos: call_pos,
                };
            }
        }
        Place::Field(self.member_on(
            obj,
            name,
            prop_pos,
            Some(matches!(m.object, ast::Expression::ThisExpression(_))),
            fx,
        ))
    }

    fn check_namespace_place(
        &mut self,
        obj: &ast::Expression,
        prop: &str,
        prop_pos: Pos,
        fx: &mut FnCtx<'p>,
    ) -> Option<Place> {
        let ast::Expression::Identifier(ident) = obj else {
            return None;
        };
        let name = ident.name.to_string();
        if fx.owns_local_name(&name) {
            return None;
        }
        let receiver_pos = self.pos(ident.span);
        let Some(ScopeItem::Class(class)) = self.scope_item(&name, &receiver_pos, fx) else {
            return self
                .check_namespace_member(obj, prop, prop_pos, fx, true)
                .map(Place::StaticField);
        };
        if self.reject_member_access(class, prop, true, true, fx, prop_pos.clone()) {
            return Some(Place::StaticField(self.err_expr(prop_pos)));
        }
        let class_name = self.classes[class.0].name.clone();
        if let Some(signature) = self.class_sigs[class.0].static_fields.get(prop).cloned() {
            if self.diagnosed_class_accesses.contains(&receiver_pos) {
                self.diagnosed_class_accesses.push(prop_pos.clone());
            }
            let symbol = static_member_symbol(class, &class_name, prop);
            if !signature.mutable {
                self.reject_subset(
                    RejectionSite::ConstStaticFieldAssignment,
                    format!("cannot rebind `const` binding `{class_name}.{prop}`"),
                    prop_pos.clone(),
                );
            }
            let ty = self.decide_global(&symbol, prop_pos.clone());
            return Some(Place::StaticField(hir::Expr {
                pending_work: None,
                kind: ExprKind::Global(hir::Symbol::from_full_text(symbol), true),
                ty,
                pos: prop_pos,
            }));
        }
        if self.class_sigs[class.0].has_static_accessor(prop) {
            let Some(signature) = self.class_sigs[class.0].static_methods.get(prop) else {
                self.reject_subset(
                    RejectionSite::WriteStaticSetterOnlyAccessor,
                    format!("static read accessor `{class_name}.{prop}` is missing"),
                    prop_pos.clone(),
                );
                return Some(Place::Accessor {
                    class,
                    receiver: None,
                    name: prop.to_string(),
                    ty: Type::Error,
                    pos: prop_pos,
                });
            };
            return Some(Place::Accessor {
                class,
                receiver: None,
                name: prop.to_string(),
                ty: signature.ret.clone(),
                pos: prop_pos,
            });
        }
        self.check_namespace_member(obj, prop, prop_pos, fx, true)
            .map(Place::StaticField)
    }
}

/// Returns the local array that an element store writes into, through
/// element reads only. A path through a field or a global has no root.
fn local_element_root(target: &hir::Expr) -> Option<&str> {
    match &target.kind {
        ExprKind::Index { obj, .. } => match &obj.kind {
            ExprKind::Local(name, _, _) => Some(name),
            ExprKind::Index { .. } => local_element_root(obj),
            _ => None,
        },
        _ => None,
    }
}

// Keep the numeric value, rather than the spelling (0 and 0.0 are the same literal).
#[derive(Debug, PartialEq)]
enum NumericOrBooleanLiteral {
    Number(f64),
    Boolean(bool),
}

fn numeric_or_boolean_literal(source: &ast::Expression) -> Option<NumericOrBooleanLiteral> {
    match super::unparen_expr(source) {
        ast::Expression::NumericLiteral(n) => Some(NumericOrBooleanLiteral::Number(n.value)),
        ast::Expression::BooleanLiteral(b) => Some(NumericOrBooleanLiteral::Boolean(b.value)),
        ast::Expression::UnaryExpression(u) if u.operator == ast::UnaryOperator::UnaryNegation => {
            match numeric_or_boolean_literal(&u.argument)? {
                NumericOrBooleanLiteral::Number(n) => Some(NumericOrBooleanLiteral::Number(-n)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn numeric_or_boolean_literal_spelling(source: &ast::Expression) -> Option<String> {
    match super::unparen_expr(source) {
        ast::Expression::NumericLiteral(n) => Some(
            n.raw
                .as_ref()
                .map_or_else(|| n.value.to_string(), |raw| raw.to_string()),
        ),
        ast::Expression::BooleanLiteral(b) => Some(b.value.to_string()),
        ast::Expression::UnaryExpression(u) if u.operator == ast::UnaryOperator::UnaryNegation => {
            Some(format!(
                "-{}",
                numeric_or_boolean_literal_spelling(&u.argument)?
            ))
        }
        _ => None,
    }
}
