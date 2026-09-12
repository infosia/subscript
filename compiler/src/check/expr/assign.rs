//! Checks assignment expressions and the places that they write.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{static_member_symbol, Checker, FnCtx, ScopeItem};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
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
            if a.op == A::NullishAssign {
                self.error_diverging(
                    RuleCode::S100,
                    "assignment operator outside the decided surface",
                    pos.clone(),
                    Divergence::NullishAssignment,
                );
            } else {
                self.error(
                    RuleCode::S100,
                    "assignment operator outside the decided surface",
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        };
        // §107.3: a pattern binds new names. A pattern that writes
        // existing targets carries its own reason.
        if let ast::AssignTarget::Pat(target) = &a.left {
            self.error_diverging(
                RuleCode::S100,
                "a destructuring assignment needs an evaluation and write order for its targets; a binding pattern declares its names",
                self.pos(target.span()),
                Divergence::AssignmentPattern,
            );
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
        place.record_kind();
        let target_ty = place.ty().clone();
        let value_ctx = if matches!(target_ty, Type::Error) {
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
                self.error_diverging(
                    RuleCode::S100,
                    format!("{spelling} cannot be used as a value"),
                    pos.clone(),
                    Divergence::ClassIndexSignature,
                );
                return self.err_expr(pos);
            }
            if signature.readonly {
                self.error(
                    RuleCode::S100,
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
            self.require_assignable(
                &value.ty.clone(),
                &signature.element_ty,
                value.pos.clone(),
                "the assignment",
            );
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(receiver),
                        name: "set".to_string(),
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
                self.error_diverging(
                    RuleCode::S100,
                    format!("{spelling} cannot be used as a value"),
                    pos.clone(),
                    Divergence::NamedAccessor,
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
                self.error(
                    RuleCode::S100,
                    format!("{spelling} cannot write through a read-only accessor"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let Some(parameter) = signature.params.first() else {
                self.error(
                    RuleCode::S100,
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
            self.require_assignable(
                &value.ty.clone(),
                &parameter.ty,
                value.pos.clone(),
                "the assignment",
            );
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Func(static_member_symbol(&class_name, &write_name)),
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
                self.error_diverging(
                    RuleCode::S100,
                    format!("{spelling} cannot be used as a value"),
                    pos.clone(),
                    Divergence::NamedAccessor,
                );
                return self.err_expr(pos);
            }
            let write_name = format!("{name}=");
            let Some(signature) = self.class_sigs[id.0].methods.get(&write_name).cloned() else {
                let target = format!("x.{name}");
                let spelling = write_spelling(WriteExpression::Assign(a), &target);
                self.error(
                    RuleCode::S100,
                    format!("{spelling} cannot write through a read-only accessor"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            };
            let Some(parameter) = signature.params.first() else {
                self.error(
                    RuleCode::S100,
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
            self.require_assignable(
                &value.ty.clone(),
                &parameter.ty,
                value.pos.clone(),
                "the assignment",
            );
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(recv),
                        name: write_name,
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
            target_ty.clone()
        };
        self.require_assignable(
            &value.ty.clone(),
            &target_ty,
            value.pos.clone(),
            "the assignment",
        );
        if op.is_none() && target_ty.carries_async_handle() {
            let origins = self.expr_async_origins(&value, fx);
            match &target.kind {
                ExprKind::Local(name) => fx.set_local_async_origins(name, origins),
                ExprKind::Index { obj, .. } => {
                    if let ExprKind::Local(name) = &obj.kind {
                        let mut stored = fx.local_async_origins(name);
                        stored.extend(origins);
                        fx.set_local_async_origins(name, stored);
                    }
                }
                _ => {}
            }
        }
        // C5 escape rule: capturing lambdas may not be stored.
        match &target.kind {
            ExprKind::Local(name) => {
                if self.is_capturing_value(&value, fx) {
                    let name = name.clone();
                    fx.taint_capturing(&name);
                }
            }
            ExprKind::Global(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => {
                if self.is_capturing_value(&value, fx) {
                    self.error(
                        RuleCode::S009,
                        "capturing lambdas may not escape: they cannot be stored in \
                         globals, fields, or arrays",
                        value.pos.clone(),
                    );
                }
            }
            _ => {}
        }
        // C7: an assignment invalidates narrowing for the path and its
        // extensions.
        if let Some(key) = path_key(&target) {
            let prefix = format!("{}.", key);
            fx.narrowed.retain(|k| k != &key && !k.starts_with(&prefix));
        }
        hir::Expr {
            kind: ExprKind::Assign {
                op,
                target: Box::new(target),
                value: Box::new(value),
            },
            ty: result_ty,
            pos,
        }
    }

    pub(super) fn check_assign_target(
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
                        self.error(
                            RuleCode::S100,
                            format!("cannot rebind `const` binding `{}`", name),
                            ident_pos.clone(),
                        );
                    }
                    return Place::Local(hir::Expr {
                        kind: ExprKind::Local(name),
                        ty: local.ty,
                        pos: ident_pos,
                    });
                }
                if let Some(ScopeItem::Global(g)) = self.scope_item(&name) {
                    let sig = self.global_sigs.get(&g).cloned();
                    if let Some(sig) = sig {
                        if !sig.mutable {
                            self.error(
                                RuleCode::S100,
                                format!("cannot rebind `const` binding `{}`", name),
                                ident_pos.clone(),
                            );
                        }
                        return Place::Global(hir::Expr {
                            kind: ExprKind::Global(g),
                            ty: sig.ty,
                            pos: ident_pos,
                        });
                    }
                }
                if matches!(self.scope_item(&name), Some(ScopeItem::Poisoned)) {
                    return Place::Local(self.err_expr(ident_pos));
                }
                self.error(
                    RuleCode::S100,
                    format!("`{}` is not an assignable binding", name),
                    ident_pos.clone(),
                );
                Place::Local(self.err_expr(ident_pos))
            }
            PlaceSource::Member(member) => self.check_member_place(member, fx),
            PlaceSource::Unsupported => {
                self.error(
                    RuleCode::S100,
                    "assignment target outside the decided surface",
                    pos.clone(),
                );
                Place::Local(self.err_expr(pos.clone()))
            }
        }
    }

    fn check_member_place(&mut self, m: &ast::MemberExpr, fx: &mut FnCtx) -> Place {
        let pos = self.pos(m.span);
        match &m.prop {
            ast::MemberProp::Computed(c) => {
                let obj = self.check_receiver(&m.obj, fx);
                let index_context = match &obj.ty {
                    Type::Class(id) => self.classes[id.0]
                        .index_signature
                        .as_ref()
                        .map(|signature| signature.index_ty.clone())
                        .unwrap_or(Type::I32),
                    _ => Type::I32,
                };
                let index = self.check_expr(&c.expr, Some(&index_context), fx);
                if let Type::Class(id) = &obj.ty {
                    if let Some(signature) = self.classes[id.0].index_signature.clone() {
                        self.require_assignable(
                            &index.ty.clone(),
                            &signature.index_ty,
                            index.pos.clone(),
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
                Place::Index(self.check_index(obj, index, pos))
            }
            ast::MemberProp::Ident(prop) => {
                let name = prop.sym.to_string();
                let prop_pos = self.pos(prop.span);
                if let Some(place) = self.check_namespace_place(&m.obj, &name, prop_pos.clone(), fx)
                {
                    return place;
                }
                let obj = self.check_receiver(&m.obj, fx);
                if let Type::Class(id) = &obj.ty {
                    if self.classes[id.0]
                        .fields
                        .iter()
                        .any(|field| field.name == name)
                    {
                        return Place::Field(self.member_on(obj, &name, prop_pos, true));
                    }
                    if self.class_sigs[id.0].has_accessor(&name) {
                        let Some(signature) = self.class_sigs[id.0].methods.get(&name) else {
                            self.error(
                                RuleCode::S100,
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
                Place::Field(self.member_on(obj, &name, prop_pos, true))
            }
            ast::MemberProp::PrivateName(_) => {
                self.error(
                    RuleCode::S100,
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
        let Some(ScopeItem::Class(class)) = self.scope_item(&name) else {
            return self
                .check_namespace_member(obj, prop, prop_pos, fx, true)
                .map(Place::StaticField);
        };
        let class_name = self.classes[class.0].name.clone();
        if let Some(signature) = self.class_sigs[class.0].static_fields.get(prop).cloned() {
            let symbol = static_member_symbol(&class_name, prop);
            if !signature.mutable {
                self.error(
                    RuleCode::S100,
                    format!("cannot rebind `const` binding `{symbol}`"),
                    prop_pos.clone(),
                );
            }
            return Some(Place::StaticField(hir::Expr {
                kind: ExprKind::Global(symbol),
                ty: signature.ty,
                pos: prop_pos,
            }));
        }
        if self.class_sigs[class.0].has_static_accessor(prop) {
            let Some(signature) = self.class_sigs[class.0].static_methods.get(prop) else {
                self.error(
                    RuleCode::S018,
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
