//! Checks assignment expressions and the places that they write.

use crate::check::rejection::RejectionSite;
use swc_common::Spanned;
use swc_ecma_ast as ast;

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
        a: &ast::AssignExpr,
        fx: &mut FnCtx,
        pos: Pos,
        statement_position: bool,
        mut prefix: Option<&mut crate::check::SyntheticPrefix>,
    ) -> hir::Expr {
        use ast::AssignOp as A;
        let operation = assign_op(a.op);
        let op = if a.op == A::Assign {
            None
        } else if let Some((op, _)) = operation {
            Some(op)
        } else {
            if self.reject_readonly_assignment_target(&a.left, fx) {
                return self.err_expr(pos);
            }
            if a.op == A::NullishAssign {
                self.reject_subset(
                    RejectionSite::UnsignedShiftAssignment,
                    "assignment operator outside the decided surface",
                    pos.clone(),
                );
            } else {
                self.reject_subset(
                    RejectionSite::LogicalOrPowerAssignment,
                    "assignment operator outside the decided surface",
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        };
        // §107.3: a pattern binds new names. A pattern that writes
        // existing targets carries its own reason.
        if let ast::AssignTarget::Pat(target) = &a.left {
            if self.reject_readonly_assignment_target(&a.left, fx) {
                return self.err_expr(pos);
            }
            self.reject_subset(RejectionSite::DestructuringAssignment, "a destructuring assignment needs an evaluation and write order for its targets; a binding pattern declares its names", self.pos(target.span()));
            return self.err_expr(pos);
        }
        let source = match &a.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(binding)) => {
                PlaceSource::Ident(&binding.id)
            }
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                PlaceSource::Member(member)
            }
            _ => PlaceSource::Unsupported,
        };
        let place = self.check_assign_target(source, fx, &pos);
        #[cfg(test)]
        if self.narrowing_analysis.is_some() {
            place.record_kind();
        }
        let target_ty = place.ty().clone();
        let value_ctx = if matches!(self.apparent_type(&target_ty), Type::Error) {
            None
        } else {
            Some(target_ty.clone())
        };
        let value = self.check_expr(&a.right, value_ctx.as_ref(), fx);
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
            let (receiver, index, value) = if let Some((bin, _)) = operation {
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
            self.require_expr_assignable(&value, &signature.element_ty, fx, "the assignment");
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
            let value = if let Some((bin, _)) = operation {
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
            self.require_expr_assignable(&value, parameter.ty(), fx, "the assignment");
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
            let Some(parameter) = signature.params.first() else {
                self.reject_subset(
                    RejectionSite::SetterParameterMissing,
                    format!("write accessor `{name}` has no parameter signature"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let (recv, value) = if let Some((bin, _)) = operation {
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
            self.require_expr_assignable(&value, parameter.ty(), fx, "the assignment");
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
            result.expr.ty
        } else {
            value.ty.clone()
        };
        if op.is_none() {
            self.require_expr_assignable(&value, &target_ty, fx, "the assignment");
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
        if op.is_none() && self.apparent_type(&target_ty).carries_async_handle() {
            let origins = self.expr_async_origins(&value, fx);
            match &target.kind {
                ExprKind::Local(name, _, _) => fx.set_local_async_origins(name, origins),
                ExprKind::Index { obj, .. } => {
                    if let ExprKind::Local(name, _, _) = &obj.kind {
                        let mut stored = fx.local_async_origins(name);
                        stored.extend(origins);
                        fx.set_local_async_origins(name, stored);
                    }
                }
                _ => {}
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

    pub(super) fn check_assign_target(
        &mut self,
        target: PlaceSource<'_>,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Place {
        let previous = fx
            .field_initializer
            .as_ref()
            .is_some_and(|context| context.write);
        let writes_this = match &target {
            PlaceSource::Member(member) => {
                let mut object = super::unparen_expr(&member.obj);
                loop {
                    object = match object {
                        ast::Expr::Member(member) => super::unparen_expr(&member.obj),
                        ast::Expr::TsAs(cast) => super::unparen_expr(&cast.expr),
                        _ => break,
                    };
                }
                matches!(object, ast::Expr::This(_))
            }
            _ => false,
        };
        if let Some(context) = &mut fx.field_initializer {
            context.write = writes_this;
        }
        let place = self.check_assign_target_inner(target, fx, pos);
        if let Some(context) = &mut fx.field_initializer {
            context.write = previous;
        }
        place
    }

    fn check_assign_target_inner(
        &mut self,
        target: PlaceSource<'_>,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Place {
        match target {
            PlaceSource::Ident(ident) => {
                let name = ident.sym.to_string();
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
                let item = self.scope_item(&name, &ident_pos);
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
                            kind: ExprKind::Global(hir::Symbol::from_full_text(g)),
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

    fn check_write_receiver(&mut self, member: &ast::MemberExpr, fx: &mut FnCtx) -> hir::Expr {
        // Resolve a readonly target through the enclosing method receiver.
        // The modifier check retains the arrow frame and rejects the write.
        let enclosing_this = if matches!(super::unparen_expr(&member.obj), ast::Expr::This(_))
            && fx
                .frames
                .last()
                .is_some_and(|frame| frame.is_lambda && frame.this_ty.is_none())
        {
            fx.frames.iter().rev().find_map(|frame| frame.this_ty.clone()).filter(|ty| {
                matches!((self.apparent_type(ty), &member.prop), (Type::Class(class), ast::MemberProp::Ident(name))
                    if self.is_readonly_field(class, name.sym.as_ref()))
            })
        } else {
            None
        };
        if let Some(ty) = enclosing_this {
            if let Some(frame) = fx.frames.last_mut() {
                frame.this_ty = Some(ty);
            }
            let receiver = self.check_receiver(&member.obj, fx);
            if let Some(frame) = fx.frames.last_mut() {
                frame.this_ty = None;
            }
            receiver
        } else {
            self.check_receiver(&member.obj, fx)
        }
    }

    fn reject_readonly_assignment_target(
        &mut self,
        target: &ast::AssignTarget,
        fx: &mut FnCtx,
    ) -> bool {
        fn collect<'a>(pattern: &'a ast::Pat, members: &mut Vec<&'a ast::MemberExpr>) {
            match pattern {
                ast::Pat::Expr(expression) => {
                    if let ast::Expr::Member(member) = super::unparen_expr(expression) {
                        members.push(member);
                    }
                }
                ast::Pat::Array(array) => {
                    for element in array.elems.iter().flatten() {
                        collect(element, members);
                    }
                }
                ast::Pat::Object(object) => {
                    for property in &object.props {
                        match property {
                            ast::ObjectPatProp::KeyValue(property) => {
                                collect(&property.value, members)
                            }
                            ast::ObjectPatProp::Rest(rest) => collect(&rest.arg, members),
                            ast::ObjectPatProp::Assign(_) => {}
                        }
                    }
                }
                ast::Pat::Assign(assignment) => collect(&assignment.left, members),
                ast::Pat::Rest(rest) => collect(&rest.arg, members),
                ast::Pat::Ident(_) | ast::Pat::Invalid(_) => {}
            }
        }
        let pattern;
        let mut members = Vec::new();
        match target {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                members.push(member)
            }
            ast::AssignTarget::Pat(target) => {
                pattern = match target {
                    ast::AssignTargetPat::Array(array) => ast::Pat::Array(array.clone()),
                    ast::AssignTargetPat::Object(object) => ast::Pat::Object(object.clone()),
                    ast::AssignTargetPat::Invalid(_) => return false,
                };
                collect(&pattern, &mut members);
            }
            _ => return false,
        }
        for member in members {
            let ast::MemberProp::Ident(name) = &member.prop else {
                continue;
            };
            let receiver = self.check_write_receiver(member, fx);
            if let Type::Class(class) = self.apparent_type(&receiver.ty) {
                if self.reject_readonly_field_write(
                    class,
                    name.sym.as_ref(),
                    matches!(&*member.obj, ast::Expr::This(_)),
                    fx,
                    self.pos(name.span),
                ) {
                    return true;
                }
            }
        }
        false
    }

    fn check_member_place(&mut self, m: &ast::MemberExpr, fx: &mut FnCtx) -> Place {
        let pos = self.pos(m.span);
        if self.reject_static_this_member(m, fx) {
            return Place::Field(self.err_expr(pos));
        }
        match &m.prop {
            ast::MemberProp::Computed(c) => {
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
                let index = self.check_expr(&c.expr, Some(&index_context), fx);
                if let Some(context) = &mut fx.field_initializer {
                    context.write = previous;
                }
                if let Type::Class(id) = &self.apparent_type(&obj.ty) {
                    if let Some(signature) = self.classes[id.0].index_signature.clone() {
                        self.require_expr_assignable(&index, &signature.index_ty, fx, "the index");
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
            ast::MemberProp::Ident(prop) => {
                let name = prop.sym.to_string();
                let prop_pos = self.pos(prop.span);
                if let Some(place) = self.check_namespace_place(&m.obj, &name, prop_pos.clone(), fx)
                {
                    return place;
                }
                let obj = self.check_write_receiver(m, fx);
                if let Type::Class(id) = &self.apparent_type(&obj.ty) {
                    if self.classes[id.0]
                        .fields
                        .iter()
                        .any(|field| field.name == name)
                    {
                        return Place::Field(self.member_on(
                            obj,
                            &name,
                            prop_pos,
                            Some(matches!(&*m.obj, ast::Expr::This(_))),
                            fx,
                        ));
                    }
                    if self.class_sigs[id.0].has_accessor(&name) {
                        if self.reject_member_access(*id, &name, false, true, fx, prop_pos.clone())
                        {
                            return Place::Field(self.err_expr(prop_pos));
                        }
                        let Some(signature) = self.class_sigs[id.0].methods.get(&name) else {
                            self.reject_subset(
                                RejectionSite::WriteSetterOnlyAccessor,
                                format!("read accessor `{name}` has no checker signature"),
                                prop_pos.clone(),
                            );
                            return Place::Accessor {
                                class: *id,
                                receiver: Some(obj),
                                name,
                                ty: Type::Error,
                                pos: prop_pos,
                            };
                        };
                        let ty = signature.ret.clone();
                        let call_pos = obj.pos.clone();
                        return Place::Accessor {
                            class: *id,
                            receiver: Some(obj),
                            name,
                            ty,
                            pos: call_pos,
                        };
                    }
                }
                Place::Field(self.member_on(
                    obj,
                    &name,
                    prop_pos,
                    Some(matches!(&*m.obj, ast::Expr::This(_))),
                    fx,
                ))
            }
            ast::MemberProp::PrivateName(_) => {
                self.reject_subset(
                    RejectionSite::PrivateMemberAssignment,
                    "private names are not in the decided surface",
                    pos.clone(),
                );
                Place::Field(self.err_expr(pos))
            }
        }
    }

    fn check_namespace_place(
        &mut self,
        obj: &ast::Expr,
        prop: &str,
        prop_pos: Pos,
        fx: &mut FnCtx,
    ) -> Option<Place> {
        let ast::Expr::Ident(ident) = obj else {
            return None;
        };
        let name = ident.sym.to_string();
        if fx.owns_local_name(&name) {
            return None;
        }
        let receiver_pos = self.pos(ident.span);
        let Some(ScopeItem::Class(class)) = self.scope_item(&name, &receiver_pos) else {
            return self
                .check_namespace_member(obj, prop, prop_pos, fx, true)
                .map(Place::StaticField);
        };
        if self.reject_member_access(class, prop, true, true, fx, prop_pos.clone()) {
            return Some(Place::StaticField(self.err_expr(prop_pos)));
        }
        let class_name = self.classes[class.0].name.clone();
        if let Some(signature) = self.class_sigs[class.0].static_fields.get(prop).cloned() {
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
                kind: ExprKind::Global(hir::Symbol::from_full_text(symbol)),
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
