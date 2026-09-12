//! Checks member reads and index reads.

use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, Callee, ExprKind, MapFn, RegexFn, SetFn};
use crate::types::Type;

use super::path_key;

impl<'p> Checker<'p> {
    pub(super) fn check_member_read(&mut self, m: &ast::MemberExpr, fx: &mut FnCtx) -> hir::Expr {
        self.check_member_read_inner(m, fx, false)
    }

    pub(super) fn check_member_read_inner(
        &mut self,
        m: &ast::MemberExpr,
        fx: &mut FnCtx,
        allow_absence_test: bool,
    ) -> hir::Expr {
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
                self.check_index(obj, index, pos)
            }
            ast::MemberProp::Ident(prop) => {
                let name = prop.sym.to_string();
                let prop_pos = self.pos(prop.span);
                if let Some(handled) =
                    self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx, false)
                {
                    return handled;
                }
                let obj = self.check_receiver(&m.obj, fx);
                let mut expr = self.member_on(obj, &name, prop_pos, false);
                self.apply_narrowing(&mut expr, fx);
                let narrowed = path_key(&expr).is_some_and(|key| fx.narrowed.contains(&key));
                if self.is_absence_capable_member_expr(&expr) && !allow_absence_test && !narrowed {
                    self.error_diverging(
                        RuleCode::S100,
                        "an absence-capable descriptor member may be read only after an `!== undefined` presence test",
                        expr.pos.clone(),
                        Divergence::OptionalDescriptorMember,
                    );
                    expr.ty = Type::Error;
                }
                expr
            }
            ast::MemberProp::PrivateName(_) => {
                self.error(
                    RuleCode::S100,
                    "private names are not in the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    pub(super) fn is_absence_capable_member_expr(&self, expr: &hir::Expr) -> bool {
        let ExprKind::Field { obj, name } = &expr.kind else {
            return false;
        };
        let Type::Class(class) = &obj.ty else {
            return false;
        };
        self.classes[class.0]
            .fields
            .iter()
            .any(|field| field.name == name.as_str() && field.is_absence_capable)
    }

    pub(in crate::check) fn check_index(
        &mut self,
        obj: hir::Expr,
        index: hir::Expr,
        pos: Pos,
    ) -> hir::Expr {
        if let Type::Class(id) = &obj.ty {
            if let Some(signature) = self.classes[id.0].index_signature.clone() {
                self.require_assignable(
                    &index.ty.clone(),
                    &signature.index_ty,
                    index.pos.clone(),
                    "the index",
                );
                return hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Method {
                            recv: Box::new(obj),
                            name: "get".to_string(),
                        },
                        args: vec![index],
                    },
                    ty: signature.element_ty,
                    pos,
                };
            }
        }
        let elem = match &obj.ty {
            Type::Array(t) => {
                if !matches!(index.ty, Type::I32 | Type::Error) {
                    let name = self.type_name(&index.ty);
                    self.error(
                        RuleCode::S100,
                        format!("array indices are `i32`, got `{}`", name),
                        index.pos.clone(),
                    );
                }
                (**t).clone()
            }
            Type::FixedArray(t, n) => {
                if !matches!(index.ty, Type::I32 | Type::Error) {
                    let name = self.type_name(&index.ty);
                    self.error(
                        RuleCode::S100,
                        format!("array indices are `i32`, got `{}`", name),
                        index.pos.clone(),
                    );
                }
                if let ExprKind::Int(k) = index.kind {
                    if k < 0 || k >= i64::from(*n) {
                        self.error(
                            RuleCode::S100,
                            format!("index {} out of bounds for FixedArray length {}", k, n),
                            index.pos.clone(),
                        );
                    }
                }
                (**t).clone()
            }
            Type::Error => Type::Error,
            other => {
                let name = self.type_name(other);
                self.error(
                    RuleCode::S100,
                    format!("type `{}` is not indexable", name),
                    pos.clone(),
                );
                Type::Error
            }
        };
        hir::Expr {
            kind: ExprKind::Index {
                obj: Box::new(obj),
                index: Box::new(index),
                checked: true,
            },
            ty: elem,
            pos,
        }
    }

    /// Member lookup on a checked receiver. `for_write` selects the
    /// write-side diagnostics (S004 for undeclared class properties).
    pub(in crate::check) fn member_on(
        &mut self,
        obj: hir::Expr,
        name: &str,
        prop_pos: Pos,
        for_write: bool,
    ) -> hir::Expr {
        if name == "prototype" {
            self.error_diverging(
                RuleCode::S003,
                "no prototype mutation",
                prop_pos.clone(),
                Divergence::DynamicObjectModel,
            );
            return self.err_expr(prop_pos);
        }
        match obj.ty.clone() {
            Type::Error => self.err_expr(prop_pos),
            Type::Class(id) => {
                let field = self.classes[id.0]
                    .fields
                    .iter()
                    .find(|f| f.name == name)
                    .map(|f| f.ty.clone());
                if let Some(ty) = field {
                    if !for_write
                        && name == "value"
                        && self.json_result_value_type(&Type::Class(id)).is_some()
                    {
                        return hir::Expr {
                            kind: ExprKind::JsonResultValue(Box::new(obj)),
                            ty,
                            pos: prop_pos,
                        };
                    }
                    return hir::Expr {
                        kind: ExprKind::Field {
                            obj: Box::new(obj),
                            name: name.to_string(),
                        },
                        ty,
                        pos: prop_pos,
                    };
                }
                if self.class_sigs[id.0].has_accessor(name) {
                    let Some(sig) = self.class_sigs[id.0].methods.get(name) else {
                        self.error(
                            RuleCode::S100,
                            format!("read accessor `{name}` has no checker signature"),
                            prop_pos.clone(),
                        );
                        return self.err_expr(prop_pos);
                    };
                    let call_pos = obj.pos.clone();
                    return hir::Expr {
                        kind: ExprKind::Call {
                            callee: Callee::Method {
                                recv: Box::new(obj),
                                name: name.to_string(),
                            },
                            args: Vec::new(),
                        },
                        ty: sig.ret.clone(),
                        pos: call_pos,
                    };
                }
                let class_name = self.classes[id.0].name.clone();
                if !self.class_sigs[id.0].has_member(name)
                    && self.class_sigs[id.0].has_static_member(name)
                {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{class_name}.{name}` is static and must be accessed through the class name"
                        ),
                        prop_pos.clone(),
                    );
                    return self.err_expr(prop_pos);
                }
                if for_write {
                    self.error(
                        RuleCode::S004,
                        format!(
                            "nominal types are closed: `{}` has no property `{}`",
                            class_name, name
                        ),
                        prop_pos.clone(),
                    );
                } else if let Some(sig) = self.class_sigs[id.0].methods.get(name) {
                    self.error(
                        RuleCode::S100,
                        if sig.is_async {
                            format!(
                                "async method `{name}` is not a first-class value; call it directly in await position"
                            )
                        } else {
                            format!("method `{name}` may only be called, not read as a value")
                        },
                        prop_pos.clone(),
                    );
                } else if let Some(template) = self.class_sigs[id.0].generic_methods.get(name) {
                    self.error(
                        RuleCode::S100,
                        if template.function.is_async {
                            format!(
                                "async method `{name}` is not a first-class value; call it directly in await position"
                            )
                        } else {
                            format!("method `{name}` may only be called, not read as a value")
                        },
                        prop_pos.clone(),
                    );
                } else {
                    self.error(
                        RuleCode::S018,
                        format!("`{}` has no member `{}`", class_name, name),
                        prop_pos.clone(),
                    );
                }
                self.err_expr(prop_pos)
            }
            Type::Array(_) | Type::FixedArray(..) => {
                if name == "length" && !for_write {
                    return hir::Expr {
                        kind: ExprKind::Length(Box::new(obj)),
                        ty: Type::I32,
                        pos: prop_pos,
                    };
                }
                if matches!(obj.ty, Type::Array(_)) {
                    // A member on an array outside a call position
                    // (stdlib.md §9): the accepted members beyond
                    // `length` are all methods.
                    if !for_write
                        && (name == "push"
                            || name == "pop"
                            || crate::ambient::arr_method(name).is_some())
                    {
                        self.error(
                            RuleCode::S100,
                            format!("method `{}` may only be called, not read as a value", name),
                            prop_pos.clone(),
                        );
                    } else if !self.arr_subset_rejection(name, prop_pos.clone()) {
                        self.arr_surface_error(name, prop_pos.clone());
                    }
                } else if !for_write
                    && crate::ambient::arr_method(name).is_some_and(|f| f.fixed_symbol().is_some())
                {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` may only be called, not read as a value"),
                        prop_pos.clone(),
                    );
                } else if crate::ambient::arr_method(name).is_some() {
                    self.reject_api_form(
                        "FixedArray<T, N>",
                        "non-callback T[] methods",
                        name,
                        prop_pos.clone(),
                    );
                } else {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` is outside the FixedArray surface (length, indexing, \
                             and the Q27 callback family)",
                            name
                        ),
                        prop_pos.clone(),
                    );
                }
                self.err_expr(prop_pos)
            }
            Type::Map(_, _) => {
                if name == "size" && !for_write {
                    return hir::Expr {
                        kind: ExprKind::Call {
                            callee: Callee::Map(MapFn::Size),
                            args: vec![obj],
                        },
                        ty: Type::I32,
                        pos: prop_pos,
                    };
                }
                if !for_write && crate::ambient::map_method(name).is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` may only be called, not read as a value"),
                        prop_pos.clone(),
                    );
                } else if let Some(rejection) = crate::ambient::map_rejection(name) {
                    self.emit_api_rejection(rejection, name, prop_pos.clone());
                } else {
                    self.error(
                        RuleCode::S100,
                        format!("`Map` has no accepted member `{name}` (Q24)"),
                        prop_pos.clone(),
                    );
                }
                self.err_expr(prop_pos)
            }
            Type::Set(_) => {
                if name == "size" && !for_write {
                    return hir::Expr {
                        kind: ExprKind::Call {
                            callee: Callee::Set(SetFn::Size),
                            args: vec![obj],
                        },
                        ty: Type::I32,
                        pos: prop_pos,
                    };
                }
                if !for_write && crate::ambient::set_method(name).is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` may only be called, not read as a value"),
                        prop_pos.clone(),
                    );
                } else if let Some(rejection) = crate::ambient::set_rejection(name) {
                    self.emit_api_rejection(rejection, name, prop_pos.clone());
                } else {
                    self.error(
                        RuleCode::S100,
                        format!("`Set` has no accepted member `{name}` (Q24)"),
                        prop_pos.clone(),
                    );
                }
                self.err_expr(prop_pos)
            }
            Type::Str => {
                if name == "length" && !for_write {
                    return hir::Expr {
                        kind: ExprKind::Length(Box::new(obj)),
                        ty: Type::I32,
                        pos: prop_pos,
                    };
                }
                // A member on a string outside a call position
                // (stdlib.md §8): the accepted members beyond `length`
                // are all methods.
                if !for_write && crate::ambient::str_method(name).is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("method `{}` may only be called, not read as a value", name),
                        prop_pos.clone(),
                    );
                } else if !self.str_subset_rejection(name, prop_pos.clone()) {
                    self.str_surface_error(name, prop_pos.clone());
                }
                self.err_expr(prop_pos)
            }
            Type::RegExp => {
                if !for_write && matches!(name, "source" | "flags") {
                    let function = if name == "source" {
                        RegexFn::Source
                    } else {
                        RegexFn::Flags
                    };
                    return hir::Expr {
                        kind: ExprKind::Call {
                            callee: Callee::Regex(function),
                            args: vec![obj],
                        },
                        ty: Type::Str,
                        pos: prop_pos,
                    };
                }
                let message = if name == "lastIndex" {
                    "`RegExp.lastIndex` is rejected: mutable global-match state would drive `exec`, which is not representable (Q31)".to_string()
                } else if name == "exec" {
                    "`RegExp.exec` is rejected: its result needs an array with extra fields and a tuple type, neither of which the language has (Q31)".to_string()
                } else if matches!(name, "test" | "matchStart" | "matchEnd") {
                    format!("method `{name}` may only be called, not read as a value")
                } else {
                    format!("`RegExp` has no accepted member `{name}`")
                };
                if matches!(name, "lastIndex" | "exec") {
                    self.error_diverging(
                        RuleCode::S014,
                        message,
                        prop_pos.clone(),
                        Divergence::RegExpSubset,
                    );
                } else {
                    self.error(RuleCode::S100, message, prop_pos.clone());
                }
                self.err_expr(prop_pos)
            }
            Type::IterResult(v) => {
                if for_write {
                    self.error(
                        RuleCode::S100,
                        "coroutine step results are read-only",
                        prop_pos.clone(),
                    );
                    return self.err_expr(prop_pos);
                }
                match name {
                    "done" => hir::Expr {
                        kind: ExprKind::Field {
                            obj: Box::new(obj),
                            name: name.to_string(),
                        },
                        ty: Type::Bool,
                        pos: prop_pos,
                    },
                    "value" => hir::Expr {
                        kind: ExprKind::Field {
                            obj: Box::new(obj),
                            name: name.to_string(),
                        },
                        ty: (*v).clone(),
                        pos: prop_pos,
                    },
                    _ => {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "`{}` is not part of the coroutine step result \
                                 ({{ done, value }})",
                                name
                            ),
                            prop_pos.clone(),
                        );
                        self.err_expr(prop_pos)
                    }
                }
            }
            Type::Date => {
                // A member on a Date receiver outside a call position
                // (stdlib.md §3): the accepted members are all methods.
                if for_write {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`Date` is an immutable value; `{}` cannot be assigned (Q20)",
                            name
                        ),
                        prop_pos.clone(),
                    );
                } else if name == "getTime" || crate::ambient::date_method(name).is_some() {
                    self.error(
                        RuleCode::S014,
                        format!("`{}` may only be called, not read as a value (Q20)", name),
                        prop_pos.clone(),
                    );
                } else {
                    self.date_subset_rejection(name, prop_pos.clone());
                }
                self.err_expr(prop_pos)
            }
            ty if ty.is_numeric() => {
                let known = matches!(
                    name,
                    "toFixed" | "toPrecision" | "toExponential" | "toLocaleString" | "toString"
                );
                if known {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "numeric method `{name}` may only appear in an accepted call \
                             (Number formatting on f32/f64; Q25/Q26)"
                        ),
                        prop_pos.clone(),
                    );
                } else {
                    self.error(
                        RuleCode::S018,
                        format!("`{}` has no member `{name}`", self.type_name(&ty)),
                        prop_pos.clone(),
                    );
                }
                self.err_expr(prop_pos)
            }
            Type::Object => {
                self.error(
                    RuleCode::S100,
                    "`object` is boundary-opaque; narrow it with `as` before member access",
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
            other => {
                let type_name = self.type_name(&other);
                self.error(
                    RuleCode::S018,
                    format!("`{}` has no member `{}`", type_name, name),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
        }
    }
}
