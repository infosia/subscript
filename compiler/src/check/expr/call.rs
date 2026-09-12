//! Checks call expressions, method calls, argument lists, and `new`.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{static_member_symbol, Checker, FnCtx, ParamSig, ScopeItem};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{
    self, AmbientFn, AsyncCallee, Callee, ContextBytesFn, ExprKind, MapFn, NumFn, SetFn, WorkerFn,
};
use crate::types::{ClassId, Type};

impl<'p> Checker<'p> {
    pub(super) fn check_call(
        &mut self,
        c: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let ast::Callee::Expr(callee) = &c.callee else {
            self.error(
                RuleCode::S100,
                "call form outside the decided surface",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let mut callee: &ast::Expr = callee;
        while let ast::Expr::Paren(p) = callee {
            callee = &p.expr;
        }
        match callee {
            ast::Expr::Ident(id) => self.check_named_call(id, c, fx, pos, false),
            ast::Expr::Member(m) => self.check_method_call(m, c, ctx, fx, pos),
            other => {
                let value = self.check_expr(other, None, fx);
                self.check_indirect_call(value, c, fx, pos)
            }
        }
    }

    pub(super) fn check_named_call(
        &mut self,
        id: &ast::Ident,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        unreachable_statement: bool,
    ) -> hir::Expr {
        let name = id.sym.to_string();
        let ident_pos = self.pos(id.span);
        if let Some(local) = self.lookup_local(&name, &ident_pos, fx) {
            let callee = hir::Expr {
                kind: ExprKind::Local(name),
                ty: local.ty,
                pos: ident_pos,
            };
            return self.check_indirect_call(callee, c, fx, pos);
        }
        let item = self.scope_item(&name);
        if c.type_args.is_some()
            && matches!(item, Some(ScopeItem::Func(_)) | Some(ScopeItem::Foreign(_)))
        {
            self.error(
                RuleCode::S100,
                format!("`{}` is not generic", name),
                ident_pos.clone(),
            );
        }
        match item {
            Some(ScopeItem::Poisoned) => {
                self.check_poisoned_arguments(&c.args, fx);
                self.err_expr(pos)
            }
            Some(ScopeItem::Func(f)) => self.check_direct_call(&f, c, fx, pos),
            Some(ScopeItem::Foreign(f)) => self.check_foreign_call(&f, c, fx, pos),
            Some(ScopeItem::GenericFunc(key)) => {
                let Some(type_args) = &c.type_args else {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "generic function `{}` requires explicit type arguments",
                            name
                        ),
                        ident_pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                let resolved: Vec<Type> = type_args
                    .params
                    .iter()
                    .map(|t| self.resolve_type(t))
                    .collect();
                match self.instantiate_fn(&key, &resolved, ident_pos) {
                    Some(mono) => self.check_direct_call(&mono, c, fx, pos),
                    None => self.err_expr(pos),
                }
            }
            Some(ScopeItem::Global(g)) => {
                let ty = self
                    .global_sigs
                    .get(&g)
                    .map(|s| s.ty.clone())
                    .unwrap_or(Type::Error);
                let callee = hir::Expr {
                    kind: ExprKind::Global(g),
                    ty,
                    pos: ident_pos,
                };
                self.check_indirect_call(callee, c, fx, pos)
            }
            Some(ScopeItem::Class(_)) | Some(ScopeItem::GenericClass(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("`{}` is a class; construct it with `new`", name),
                    ident_pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::Enum(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("enum `{}` is not callable", name),
                    ident_pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::StringAlias(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("string-literal union alias `{name}` is not callable"),
                    ident_pos.clone(),
                );
                self.err_expr(pos)
            }
            None => {
                if name == "eval" {
                    self.error_diverging(
                        RuleCode::S002,
                        "no dynamic code evaluation",
                        pos.clone(),
                        Divergence::DynamicObjectModel,
                    );
                    return self.err_expr(pos);
                }
                if let Some(f) = crate::ambient::number_global(&name) {
                    return self.check_number_global_call(f, c, fx, pos, &name);
                }
                if name == "Number" {
                    self.reject_api_form("Number", "Number(value)", "Number(value)", pos.clone());
                    return self.err_expr(pos);
                }
                if name == "isNaN" || name == "isFinite" {
                    let surface = if name == "isNaN" {
                        "isNaN(value)"
                    } else {
                        "isFinite(value)"
                    };
                    self.reject_api_form("global", surface, surface, pos.clone());
                    return self.err_expr(pos);
                }
                if let Some(ambient) = crate::ambient::ambient_fn(&name) {
                    if ambient == AmbientFn::Unreachable && !unreachable_statement {
                        self.error_diverging(
                            RuleCode::S100,
                            "`unreachable()` is only legal as a call statement",
                            pos.clone(),
                            Divergence::UnreachableInValuePosition,
                        );
                        let _ = self.check_ambient_call(ambient, c, fx, pos.clone());
                        return self.err_expr(pos);
                    }
                    return self.check_ambient_call(ambient, c, fx, pos);
                }
                self.error(
                    RuleCode::S016,
                    format!("unknown function `{}`", name),
                    ident_pos,
                );
                self.err_expr(pos)
            }
        }
    }

    fn check_direct_call(
        &mut self,
        fn_name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let Some(sig) = self.fn_sigs.get(fn_name).cloned() else {
            return self.err_expr(pos);
        };
        if sig.is_async {
            let args = self.check_args(&sig.params, &c.args, fx, &pos, fn_name);
            let origin = fx.register_async_origin(pos.clone());
            return hir::Expr {
                kind: ExprKind::AsyncHandleCreate {
                    callee: AsyncCallee::Function(fn_name.to_string()),
                    args,
                    origin,
                },
                ty: Type::AsyncHandle(Box::new(sig.ret)),
                pos,
            };
        }
        if sig.is_generator && !sig.yield_known {
            self.error(
                RuleCode::S100,
                format!(
                    "generator `{}` is called before its yield type is known; \
                     declare it earlier in the program",
                    fn_name
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let args = self.check_args(&sig.params, &c.args, fx, &pos, fn_name);
        let value = hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Func(fn_name.to_string()),
                args,
            },
            ty: sig.ret,
            pos,
        };
        self.track_async_call_result(value, fx)
    }

    /// Checks a call to a foreign C-ABI function declared by an ambient
    /// mirror (§12.2). Type-checks arguments against the mapped boundary
    /// signature and emits a [`Callee::Foreign`] call, which both tiers
    /// lower to an imported C symbol.
    fn check_foreign_call(
        &mut self,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let Some(sig) = self.foreign_sigs.get(name).cloned() else {
            return self.err_expr(pos);
        };
        let args = self.check_args(&sig.params, &c.args, fx, &pos, name);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Foreign(name.to_string()),
                args,
            },
            ty: sig.ret,
            pos,
        }
    }

    fn check_ambient_call(
        &mut self,
        ambient: AmbientFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let params: Vec<ParamSig> = crate::ambient::ambient_params(ambient)
            .iter()
            .map(|t| ParamSig::positional(t.clone()))
            .collect();
        let label = if matches!(ambient, AmbientFn::Collect | AmbientFn::UnsafeDelete) {
            format!("Context.{}", ambient.name())
        } else {
            ambient.name().to_string()
        };
        let args = self.check_args(&params, &c.args, fx, &pos, &label);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Ambient(ambient),
                args,
            },
            ty: Type::Void,
            pos,
        }
    }

    fn context_bytes_storage_rejection(
        &self,
        ty: &Type,
        path: Option<&str>,
        visiting: &mut std::collections::HashSet<ClassId>,
    ) -> Option<(Option<String>, Type, &'static str)> {
        if ty.is_numeric() || matches!(ty, Type::Bool | Type::Enum(_) | Type::Error) {
            return None;
        }
        match ty {
            Type::FixedArray(element, _) => {
                self.context_bytes_storage_rejection(element, path, visiting)
            }
            Type::Class(id) => {
                let class = self.classes.get(id.0)?;
                if !class.is_value {
                    return Some((
                        path.map(str::to_string),
                        ty.clone(),
                        "it is a reference class",
                    ));
                }
                if !visiting.insert(*id) {
                    return None;
                }
                for field in &class.fields {
                    let nested = path.map_or_else(
                        || field.name.clone(),
                        |prefix| format!("{prefix}.{}", field.name),
                    );
                    if let Some(rejection) =
                        self.context_bytes_storage_rejection(&field.ty, Some(&nested), visiting)
                    {
                        visiting.remove(id);
                        return Some(rejection);
                    }
                }
                visiting.remove(id);
                class.is_boundary.then(|| {
                    (
                        path.map(str::to_string),
                        ty.clone(),
                        "it is a boundary struct",
                    )
                })
            }
            other => Some((
                path.map(str::to_string),
                other.clone(),
                "its storage type is not eligible",
            )),
        }
    }

    fn check_context_bytes_call(
        &mut self,
        function: ContextBytesFn,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        member_pos: Pos,
    ) -> hir::Expr {
        let name = function.name();
        let Some(type_args) = &call.type_args else {
            self.error(
                RuleCode::S014,
                format!("`Context.{name}<T>` takes exactly one type argument"),
                member_pos,
            );
            return self.err_expr(pos);
        };
        if type_args.params.len() != 1 {
            self.error(
                RuleCode::S014,
                format!("`Context.{name}<T>` takes exactly one type argument"),
                member_pos,
            );
            return self.err_expr(pos);
        }
        let target = self.resolve_type(&type_args.params[0]);
        if target == Type::Error {
            return self.err_expr(pos);
        }
        let top_level_ok = match &target {
            Type::FixedArray(..) => true,
            Type::Class(id) => self.classes.get(id.0).is_some_and(|class| class.is_value),
            _ => false,
        };
        if !top_level_ok {
            let target_name = self.type_name(&target);
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "`Context.{name}<T>` cannot use `{target_name}`; it is not a @CStruct value class or FixedArray"
                ),
                member_pos,
                Divergence::ByteAccessTarget,
            );
            return self.err_expr(pos);
        }
        let rejection = self.context_bytes_storage_rejection(
            &target,
            None,
            &mut std::collections::HashSet::new(),
        );
        if let Some((field, leaf, reason)) = rejection {
            let target_name = self.type_name(&target);
            let detail = field.map_or_else(
                || {
                    format!(
                        "{reason}; unsupported storage type is `{}`",
                        self.type_name(&leaf)
                    )
                },
                |field| {
                    format!(
                        "field `{field}` has unsupported type `{}` ({reason})",
                        self.type_name(&leaf)
                    )
                },
            );
            self.error_diverging(
                RuleCode::S100,
                format!("`Context.{name}<T>` cannot use `{target_name}`; {detail}"),
                member_pos,
                Divergence::ByteAccessTarget,
            );
            return self.err_expr(pos);
        }

        let params = match function {
            ContextBytesFn::BytesOf => vec![target.clone()],
            ContextBytesFn::BytesInto => {
                vec![target.clone(), Type::Array(Box::new(Type::U8)), Type::U32]
            }
            ContextBytesFn::FromBytes => {
                vec![Type::Array(Box::new(Type::U8)), Type::U32]
            }
        };
        if call.args.len() != params.len() {
            self.error(
                RuleCode::S014,
                format!(
                    "`Context.{name}` expects exactly {} argument(s), got {}",
                    params.len(),
                    call.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let mut args = Vec::with_capacity(params.len());
        for (argument, expected) in call.args.iter().zip(&params) {
            if let Some(spread) = argument.spread {
                let spread_pos = self.pos(spread);
                self.error(
                    RuleCode::S014,
                    "spread arguments require variadic parameters, which the language does not have",
                    spread_pos.clone(),
                );
                return self.err_expr(spread_pos);
            }
            let checked = self.check_expr(&argument.expr, Some(expected), fx);
            if checked.ty != *expected && checked.ty != Type::Error {
                self.error(
                    RuleCode::S100,
                    format!(
                        "type mismatch: `Context.{name}` expects exactly `{}`, got `{}`",
                        self.type_name(expected),
                        self.type_name(&checked.ty)
                    ),
                    checked.pos.clone(),
                );
            }
            args.push(checked);
        }
        let return_type = match function {
            ContextBytesFn::BytesOf => Type::Array(Box::new(Type::U8)),
            ContextBytesFn::BytesInto => Type::Void,
            ContextBytesFn::FromBytes => target.clone(),
        };
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::ContextBytes {
                    function,
                    ty: target,
                },
                args,
            },
            ty: return_type,
            pos,
        }
    }

    pub(super) fn check_indirect_call(
        &mut self,
        callee: hir::Expr,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        match callee.ty.clone() {
            Type::Func(ft) => {
                let params: Vec<ParamSig> = ft
                    .params
                    .iter()
                    .map(|t| ParamSig::positional(t.clone()))
                    .collect();
                let args = self.check_args(&params, &c.args, fx, &pos, "the function value");
                let value = hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Value(Box::new(callee)),
                        args,
                    },
                    ty: ft.ret.clone(),
                    pos,
                };
                self.track_async_call_result(value, fx)
            }
            Type::Error => self.err_expr(pos),
            other => {
                let name = self.type_name(&other);
                self.error(
                    RuleCode::S100,
                    format!("type `{}` is not callable", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    pub(super) fn rejected_static_generic_method(
        &self,
        receiver: &ast::Expr,
        name: &str,
        fx: &FnCtx,
    ) -> bool {
        let ast::Expr::Ident(receiver) = receiver else {
            return false;
        };
        if fx.owns_local_name(receiver.sym.as_ref()) {
            return false;
        }
        match self.scope_item(receiver.sym.as_ref()) {
            Some(ScopeItem::Class(class)) => {
                self.class_sigs[class.0].generic_method_is_rejected(name, true)
            }
            Some(ScopeItem::GenericClass(key)) => {
                self.generic_classes.get(&key).is_some_and(|class| {
                    class
                        .rejected_generic_methods
                        .contains_key(&(Some(name.to_string()), true))
                })
            }
            _ => false,
        }
    }

    fn check_static_method_call(
        &mut self,
        member: &ast::MemberExpr,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        member_pos: Pos,
        name: &str,
    ) -> Option<hir::Expr> {
        if self.rejected_static_generic_method(&member.obj, name, fx) {
            return Some(self.err_expr(pos));
        }
        let ast::Expr::Ident(receiver) = &*member.obj else {
            return None;
        };
        let class_name = receiver.sym.to_string();
        if fx.owns_local_name(&class_name) {
            return None;
        }
        let Some(ScopeItem::Class(class)) = self.scope_item(&class_name) else {
            return None;
        };
        if self.class_sigs[class.0].has_static_accessor(name) {
            return None;
        }
        // §82.4 rule 2: a static generic method instantiates at the call.
        if self.class_sigs[class.0].has_generic_method(name, true) {
            let Some(instance) =
                self.instantiate_generic_method_call(class, name, call, true, member_pos)
            else {
                return Some(self.err_expr(pos));
            };
            let symbol = static_member_symbol(&self.classes[class.0].name, &instance);
            return Some(self.check_direct_call(&symbol, call, fx, pos));
        }
        if !self.class_sigs[class.0].static_methods.contains_key(name) {
            return None;
        }
        if call.type_args.is_some() {
            self.error(
                RuleCode::S100,
                format!("static method `{class_name}.{name}` is not generic"),
                member_pos,
            );
        }
        let symbol = static_member_symbol(&self.classes[class.0].name, name);
        Some(self.check_direct_call(&symbol, call, fx, pos))
    }

    fn check_method_call(
        &mut self,
        m: &ast::MemberExpr,
        c: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let ast::MemberProp::Ident(prop) = &m.prop else {
            let value = self.check_member_read(m, fx);
            return self.check_indirect_call(value, c, fx, pos);
        };
        let name = prop.sym.to_string();
        let prop_pos = self.pos(prop.span);
        if matches!(name.as_str(), "then" | "catch" | "finally") {
            self.error_diverging(
                RuleCode::S013,
                format!("Promise combinator `.{name}(...)` is not in the language"),
                prop_pos.clone(),
                Divergence::PromiseObject,
            );
            return self.err_expr(pos);
        }
        if self.ambient_namespace(&m.obj, fx) == Some("Promise") {
            self.error_diverging(
                RuleCode::S013,
                format!("Promise static `Promise.{name}(...)` is not in the language"),
                prop_pos.clone(),
                Divergence::PromiseObject,
            );
            return self.err_expr(pos);
        }
        if matches!(&*m.obj, ast::Expr::Ident(id) if id.sym.as_ref() == "Worker")
            && self.worker_is_ambient(fx)
        {
            if name == "spawn" {
                return self.check_worker_spawn(c, fx, pos);
            }
            self.error(
                RuleCode::S018,
                format!("`Worker` has no static method `{name}`"),
                prop_pos,
            );
            return self.err_expr(pos);
        }
        if let Some(static_call) =
            self.check_static_method_call(m, c, fx, pos.clone(), prop_pos.clone(), &name)
        {
            return static_call;
        }
        // `Context.collect()` / `Context.free(value)` (Q6/Q7): ambient
        // namespace calls lower through the existing ambient-call path.
        if self.is_context_namespace(&m.obj, fx) {
            if let Some(function) = crate::ambient::context_bytes_fn(&name) {
                return self.check_context_bytes_call(function, c, fx, pos, prop_pos);
            }
            if let Some(f) = crate::ambient::context_fn(&name) {
                return self.check_ambient_call(f, c, fx, pos);
            }
        }
        // `Math.<fn>(…)` (stdlib.md §1): an ambient-namespace intrinsic
        // call, resolved before the generic namespace-member path (which
        // treats a function member read as an error). Constants and
        // out-of-subset members fall through to that path.
        if self.is_math_namespace(&m.obj, fx) {
            if let Some(f) = crate::ambient::math_fn(&name) {
                return self.check_math_call(f, c, fx, pos);
            }
        }
        // Accepted Number statics (stdlib.md §11.1/§11.3, Q27) are
        // resolved before generic namespace-member handling. Parser
        // spellings share the globals' NumFn/runtime identity.
        if self.is_number_namespace(&m.obj, fx) {
            if let Some(f) = crate::ambient::number_static(&name) {
                return if matches!(f, NumFn::ParseInt | NumFn::ParseFloat) {
                    self.check_number_global_call(f, c, fx, pos, &format!("Number.{name}"))
                } else {
                    self.check_number_predicate_call(f, c, fx, pos)
                };
            }
        }
        // `JSON.stringify<T>(value)` is checked from the argument's
        // static type and expanded into a call-site serializer graph.
        if self.is_json_namespace(&m.obj, fx) {
            return self.check_json_call(&name, c, ctx, fx, pos, prop_pos);
        }
        // `Date.UTC(…)` / `Date.now()` (stdlib.md §3): static intrinsic
        // calls, resolved before the generic namespace-member path.
        if self.is_date_namespace(&m.obj, fx) {
            if let Some(handled) = self.check_date_static_call(&name, c, fx, pos.clone()) {
                return handled;
            }
        }
        if matches!(&*m.obj, ast::Expr::Ident(id) if id.sym.as_ref() == "Map")
            && self.assoc_is_ambient("Map", fx)
            && name == "groupBy"
        {
            return self.check_map_group_by(c, fx, pos);
        }
        // `Array.<member>(…)` (compiler.md §105): `from` is the accepted
        // member; the others carry their own recorded rejection.
        if self.ambient_namespace(&m.obj, fx) == Some("Array") {
            return self.check_array_static_call(&name, c, fx, pos, prop_pos);
        }
        if let Some(handled) =
            self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx, false)
        {
            // Enum members are values, not callables.
            if matches!(handled.ty, Type::Error) {
                return handled;
            }
            return self.check_indirect_call(handled, c, fx, pos);
        }
        let recv = self.check_receiver(&m.obj, fx);
        self.check_method_call_on(recv, prop, c, ctx, fx, pos)
    }

    /// Resolves the explicit type arguments of a generic method call and
    /// instantiates the method (§82.4 rules 2 and 3).
    pub(super) fn instantiate_generic_method_call(
        &mut self,
        class: ClassId,
        name: &str,
        call: &ast::CallExpr,
        is_static: bool,
        pos: Pos,
    ) -> Option<String> {
        if self.class_sigs[class.0].generic_method_is_rejected(name, is_static) {
            return None;
        }
        let Some(type_args) = &call.type_args else {
            self.error_diverging(
                RuleCode::S100,
                format!("generic method `{name}` requires explicit type arguments"),
                pos,
                Divergence::GenericMethodTypeArguments,
            );
            return None;
        };
        let resolved: Vec<Type> = type_args
            .params
            .iter()
            .map(|ty| self.resolve_type(ty))
            .collect();
        self.instantiate_method(class, name, &resolved, is_static, pos)
    }

    /// Checks `receiver.name(...)` from an already-checked receiver.
    pub(in crate::check) fn check_method_call_on(
        &mut self,
        recv: hir::Expr,
        property: &ast::IdentName,
        c: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let mut name = property.sym.to_string();
        let prop_pos = self.pos(property.span);
        // §82.4 rule 3: the call names the instance, not the template.
        if let Type::Class(class) = &recv.ty {
            let class = *class;
            if self.class_sigs[class.0].has_generic_method(&name, false) {
                let Some(instance) =
                    self.instantiate_generic_method_call(class, &name, c, false, prop_pos.clone())
                else {
                    return self.err_expr(pos);
                };
                name = instance;
            }
        }
        let name = name;
        let mk = |recv: hir::Expr, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Method {
                    recv: Box::new(recv),
                    name: name.clone(),
                },
                args,
            },
            ty,
            pos,
        };
        match recv.ty.clone() {
            Type::Error => self.err_expr(pos),
            ty if ty.is_numeric() => self.check_number_method(recv, &name, c, fx, pos, prop_pos),
            Type::Date => self.check_date_method(recv, &name, c, fx, pos, prop_pos),
            Type::Map(key, value) => {
                self.check_map_method(recv, *key, *value, &name, c, ctx, fx, pos, prop_pos)
            }
            Type::Set(key) => self.check_set_method(recv, *key, &name, c, fx, pos, prop_pos),
            Type::Worker(input, output) => {
                let (function, params, ret) = match name.as_str() {
                    "post" => (
                        WorkerFn::Post,
                        vec![ParamSig {
                            name: "message".to_string(),
                            ty: (*input).clone(),
                            has_default: false,
                        }],
                        Type::Void,
                    ),
                    "poll" => (WorkerFn::Poll, Vec::new(), Type::Nullable(output.clone())),
                    "close" => (WorkerFn::Close, Vec::new(), Type::Void),
                    "join" => (WorkerFn::Join, Vec::new(), Type::Void),
                    _ => {
                        let type_name = self.type_name(&Type::Worker(input, output));
                        self.error(
                            RuleCode::S018,
                            format!("`{type_name}` has no method `{name}`"),
                            prop_pos,
                        );
                        return self.err_expr(pos);
                    }
                };
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, &name));
                hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Worker(function),
                        args,
                    },
                    ty: ret,
                    pos,
                }
            }
            Type::Inbox(message) => {
                let function = match name.as_str() {
                    "wait" => WorkerFn::InboxWait,
                    "poll" => WorkerFn::InboxPoll,
                    _ => {
                        let type_name = self.type_name(&Type::Inbox(message));
                        self.error(
                            RuleCode::S018,
                            format!("`{type_name}` has no method `{name}`"),
                            prop_pos,
                        );
                        return self.err_expr(pos);
                    }
                };
                let mut args = vec![recv];
                args.extend(self.check_args(&[], &c.args, fx, &pos, &name));
                hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Worker(function),
                        args,
                    },
                    ty: Type::Nullable(message),
                    pos,
                }
            }
            Type::Outbox(message) => {
                if name != "post" {
                    let type_name = self.type_name(&Type::Outbox(message));
                    self.error(
                        RuleCode::S018,
                        format!("`{type_name}` has no method `{name}`"),
                        prop_pos,
                    );
                    return self.err_expr(pos);
                }
                let params = [ParamSig {
                    name: "message".to_string(),
                    ty: (*message).clone(),
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, &name));
                hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Worker(WorkerFn::OutboxPost),
                        args,
                    },
                    ty: Type::Void,
                    pos,
                }
            }
            Type::Array(elem) => match name.as_str() {
                "push" => {
                    let params = [ParamSig::positional((*elem).clone())];
                    let args = self.check_args(&params, &c.args, fx, &pos, "push");
                    // C5: `push` stores its argument in the array.
                    for arg in &args {
                        if self.is_capturing_value(arg, fx) {
                            self.error(
                                RuleCode::S009,
                                "capturing lambdas may not escape: `push` stores its \
                                 argument in the array",
                                arg.pos.clone(),
                            );
                        }
                    }
                    mk(recv, args, Type::I32, pos)
                }
                "pop" => {
                    let args = self.check_args(&[], &c.args, fx, &pos, "pop");
                    mk(recv, args, (*elem).clone(), pos)
                }
                other => {
                    // The §9 method intrinsics (stdlib.md §9, Q22).
                    if let Some(f) = crate::ambient::arr_method(other) {
                        return self.check_array_method(recv, (*elem).clone(), f, c, fx, pos);
                    }
                    if !self.arr_subset_rejection(other, prop_pos.clone()) {
                        self.arr_surface_error(other, prop_pos.clone());
                    }
                    self.err_expr(pos)
                }
            },
            // Q27 accepts the closure-taking `every` family on the
            // in-place fixed buffer. Other checker-owned Array methods
            // retain a named S014; `push`/`pop` are not in that table and
            // keep the standing "no method" diagnostic.
            Type::FixedArray(elem, n) => {
                if let Some(f) = crate::ambient::arr_method(&name) {
                    if f.fixed_symbol().is_some() {
                        return self.check_array_method(recv, (*elem).clone(), f, c, fx, pos);
                    }
                    self.reject_api_form(
                        "FixedArray<T, N>",
                        "non-callback T[] methods",
                        &name,
                        prop_pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let type_name = self.type_name(&Type::FixedArray(elem, n));
                self.error(
                    RuleCode::S018,
                    format!("`{type_name}` has no method `{name}`"),
                    prop_pos,
                );
                self.err_expr(pos)
            }
            Type::Str => match name.as_str() {
                "search" | "replace" | "replaceAll" | "split" => {
                    self.check_string_pattern_method(recv, &name, c, fx, pos, prop_pos)
                }
                other => {
                    // The §8 method intrinsics (stdlib.md §8, Q21).
                    if let Some(f) = crate::ambient::str_method(other) {
                        return self.check_str_method(recv, f, c, fx, pos);
                    }
                    if !self.str_subset_rejection(other, prop_pos.clone()) {
                        self.str_surface_error(other, prop_pos.clone());
                    }
                    self.err_expr(pos)
                }
            },
            Type::RegExp => self.check_regex_method(recv, &name, c, fx, pos, prop_pos),
            Type::Generator(y) => match name.as_str() {
                "next" => {
                    let args = self.check_args(&[], &c.args, fx, &pos, "next");
                    let step = Type::IterResult(y.clone());
                    match crate::check::layout::class_independent_layout(&step) {
                        crate::check::layout::IndependentLayout::Fits => mk(recv, args, step, pos),
                        crate::check::layout::IndependentLayout::TooLarge => {
                            self.error_diverging(
                                RuleCode::S100,
                                format!(
                                    "coroutine step-result layout exceeds the supported \
                                     aggregate limit of {} bytes",
                                    crate::types::MAX_AGGREGATE_BYTES
                                ),
                                prop_pos,
                                Divergence::AggregateLayoutLimit,
                            );
                            self.err_expr(pos)
                        }
                        crate::check::layout::IndependentLayout::DependsOnClass => {
                            self.pending_layouts.push((
                                step.clone(),
                                prop_pos,
                                "coroutine step-result layout",
                            ));
                            mk(recv, args, step, pos)
                        }
                    }
                }
                other => {
                    self.error(
                        RuleCode::S100,
                        format!("`{}` is outside the coroutine surface (next)", other),
                        prop_pos.clone(),
                    );
                    self.err_expr(pos)
                }
            },
            Type::Class(id) => {
                let sig = self.class_sigs[id.0].methods.get(&name).cloned();
                match sig {
                    Some(sig) => {
                        if sig.is_async {
                            let args = self.check_args(&sig.params, &c.args, fx, &pos, &name);
                            let origin = fx.register_async_origin(pos.clone());
                            return hir::Expr {
                                kind: ExprKind::AsyncHandleCreate {
                                    callee: AsyncCallee::Method {
                                        class: id,
                                        receiver: Box::new(recv),
                                        name,
                                    },
                                    args,
                                    origin,
                                },
                                ty: Type::AsyncHandle(Box::new(sig.ret)),
                                pos,
                            };
                        }
                        let args = self.check_args(&sig.params, &c.args, fx, &pos, &name);
                        let value = mk(recv, args, sig.ret, pos);
                        self.track_async_call_result(value, fx)
                    }
                    None => {
                        let class_name = self.classes[id.0].name.clone();
                        if self.class_sigs[id.0].has_static_member(&name) {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "`{class_name}.{name}` is static and must be accessed through the class name"
                                ),
                                prop_pos.clone(),
                            );
                            return self.err_expr(pos);
                        }
                        self.error(
                            RuleCode::S018,
                            format!("`{}` has no method `{}`", class_name, name),
                            prop_pos.clone(),
                        );
                        self.err_expr(pos)
                    }
                }
            }
            other => {
                let type_name = self.type_name(&other);
                self.error(
                    RuleCode::S018,
                    format!("`{}` has no method `{}`", type_name, name),
                    prop_pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    pub(super) fn check_poisoned_arguments(&mut self, args: &[ast::ExprOrSpread], fx: &mut FnCtx) {
        for argument in args {
            let _ = self.check_expr(&argument.expr, None, fx);
        }
    }

    pub(super) fn check_args(
        &mut self,
        params: &[ParamSig],
        args: &[ast::ExprOrSpread],
        fx: &mut FnCtx,
        pos: &Pos,
        what: &str,
    ) -> Vec<hir::Expr> {
        let required = params.iter().filter(|p| !p.has_default).count();
        let has_spread = args.iter().any(|arg| arg.spread.is_some());
        if !has_spread && (args.len() < required || args.len() > params.len()) {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} argument(s) ({} required), got {}",
                    what,
                    params.len(),
                    required,
                    args.len()
                ),
                pos.clone(),
            );
        }
        let mut out = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            if arg.spread.is_some() {
                let p = self.pos(arg.spread.unwrap_or_default());
                self.error(
                    RuleCode::S014,
                    "spread arguments require variadic parameters, which the language does not have",
                    p,
                );
                continue;
            }
            let param_ty = params.get(i).map(|p| p.ty.clone());
            let checked = self.check_expr(&arg.expr, param_ty.as_ref(), fx);
            if let Some(param_ty) = param_ty {
                self.require_assignable(
                    &checked.ty.clone(),
                    &param_ty,
                    checked.pos.clone(),
                    "the argument",
                );
                if matches!(param_ty, Type::AsyncHandle(_) | Type::Array(_)) {
                    let origins = self.expr_async_origins(&checked, fx);
                    fx.handle_async_origins(&origins);
                }
            }
            out.push(checked);
        }
        out
    }

    /// Checks the one source operand of `new Set<K>(source)`
    /// (compiler.md §103.1). `source` is `K[]`, `FixedArray<K, N>`,
    /// `Set<K>`, or, for `Set<string>`, a `string` that yields one code
    /// point per element. Returns `None` when the form is rejected.
    fn check_set_source(
        &mut self,
        argument: &ast::ExprOrSpread,
        key: &Type,
        fx: &mut FnCtx,
    ) -> Option<hir::Expr> {
        if let Some(spread) = argument.spread {
            let spread_pos = self.pos(spread);
            self.error_diverging(
                RuleCode::S014,
                "spread arguments require variadic parameters, which the language does not have",
                spread_pos,
                Divergence::VariadicArguments,
            );
            return None;
        }
        let source = self.check_expr(&argument.expr, None, fx);
        let element = match &source.ty {
            Type::Error => return None,
            // Stock `tsc` answers TS2769 here, because `Map<K, V>` is
            // `Iterable<[K, V]>` and not `Iterable<K>`.
            Type::Map(..) => {
                self.reject_api_form("Set", "new Set(Map)", "new Set(map)", source.pos.clone());
                return None;
            }
            Type::Generator(_) => {
                self.reject_api_form(
                    "Set",
                    "new Set(Generator<T>)",
                    "new Set(generator)",
                    source.pos.clone(),
                );
                return None;
            }
            other => match other.iteration_element() {
                Some((_, element)) => element,
                None => {
                    let actual = self.type_name(other);
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`new Set(source)` accepts T[], FixedArray<T, N>, Set<T>, or \
                             string; got `{actual}`"
                        ),
                        source.pos.clone(),
                    );
                    return None;
                }
            },
        };
        self.require_assignable(&element, key, source.pos.clone(), "the Set element");
        Some(source)
    }

    pub(super) fn check_new(&mut self, n: &ast::NewExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let mut callee: &ast::Expr = &n.callee;
        while let ast::Expr::Paren(p) = callee {
            callee = &p.expr;
        }
        let ast::Expr::Ident(id) = callee else {
            self.error(RuleCode::S100, "`new` requires a class name", pos.clone());
            return self.err_expr(pos);
        };
        let name = id.sym.to_string();
        let ident_pos = self.pos(id.span);
        if fx.owns_local_name(&name) {
            if self
                .lookup_local(&name, &ident_pos, fx)
                .is_some_and(|local| !matches!(local.ty, Type::Error))
            {
                self.error(
                    RuleCode::S100,
                    format!("`{name}` names a local value here, not a class"),
                    ident_pos.clone(),
                );
            }
            return self.err_expr(pos);
        }
        if name == "Function" {
            self.error_diverging(
                RuleCode::S002,
                "no dynamic code evaluation (`new Function`)",
                pos.clone(),
                Divergence::DynamicObjectModel,
            );
            return self.err_expr(pos);
        }
        if name == "Promise" {
            self.error_diverging(
                RuleCode::S013,
                "Promise objects cannot be constructed; async functions expose no Promise object surface",
                pos.clone(),
                Divergence::PromiseObject,
            );
            return self.err_expr(pos);
        }
        if matches!(name.as_str(), "Worker" | "Inbox" | "Outbox")
            && self.scope_item(&name).is_none()
        {
            self.error(
                RuleCode::S100,
                format!(
                    "`new {name}` is rejected; Q35 worker handles and endpoints are runtime-created"
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        // compiler.md §105.3: the length constructor needs an array hole
        // and a missing-element value, and the language has neither.
        if name == "Array" && self.ambient_visible(&name, fx) {
            self.reject_api_form(
                "Array",
                "new Array(length)",
                "new Array(length)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "Number" && self.is_number_namespace(callee, fx) {
            self.reject_api_form(
                "Number",
                "new Number(value)",
                "new Number(value)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "RegExp" && self.regexp_is_ambient(fx) {
            return self.check_regex_new(n, fx, pos);
        }
        // `new Date(ms)` (stdlib.md §3): the ambient constructor applies
        // only when neither a program declaration nor a function-local
        // binding shadows the name (same resolution as member access).
        if name == "Date" && self.date_is_ambient(fx) {
            return self.check_date_new(n, fx, pos);
        }
        if (name == "Map" || name == "Set") && self.assoc_is_ambient(&name, fx) {
            let Some(type_args) = &n.type_args else {
                self.error(
                    RuleCode::S100,
                    format!("`new {name}` requires explicit type arguments (Q24)"),
                    ident_pos.clone(),
                );
                return self.err_expr(pos);
            };
            let expected = if name == "Map" { 2 } else { 1 };
            if type_args.params.len() != expected {
                self.error(
                    RuleCode::S100,
                    format!("`new {name}` takes exactly {expected} type argument(s)"),
                    ident_pos.clone(),
                );
                return self.err_expr(pos);
            }
            let arguments: &[ast::ExprOrSpread] = n.args.as_deref().unwrap_or(&[]);
            // `new Map(source)` stays rejected in every form: a pair
            // element needs a tuple type (compiler.md §103.1 rule 7).
            if name == "Map" && !arguments.is_empty() {
                self.reject_api_form("Map", "new Map(iterable)", "new Map(iterable)", pos.clone());
                return self.err_expr(pos);
            }
            let saved = self.in_assoc_key;
            self.in_assoc_key = true;
            let key = self.resolve_type(&type_args.params[0]);
            self.in_assoc_key = saved;
            if !matches!(key, Type::Error) && self.assoc_key_kind(&key).is_none() {
                let key_pos = self.pos(type_args.params[0].span());
                let key_name = self.type_name(&key);
                self.error(
                    RuleCode::S014,
                    format!("`{key_name}` is not a permitted Map/Set key kind (Q24)"),
                    key_pos,
                );
            }
            if name == "Map" {
                let value = self.resolve_type(&type_args.params[1]);
                let ty = Type::Map(Box::new(key), Box::new(value));
                return hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Map(MapFn::New),
                        args: Vec::new(),
                    },
                    ty,
                    pos,
                };
            }
            let ty = Type::Set(Box::new(key.clone()));
            // The arity guard and the source index are one match, so no
            // caller holds an emptiness precondition for the other.
            let args = match arguments {
                [] => Vec::new(),
                [argument] => match self.check_set_source(argument, &key, fx) {
                    Some(source) => vec![source],
                    None => return self.err_expr(pos),
                },
                _ => {
                    self.error(
                        RuleCode::S100,
                        "`new Set` takes at most one source argument",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
            };
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Set(SetFn::New),
                    args,
                },
                ty,
                pos,
            };
        }
        let class_id = match self.scope_item(&name) {
            Some(ScopeItem::Poisoned) => {
                if let Some(arguments) = &n.args {
                    self.check_poisoned_arguments(arguments, fx);
                }
                return self.err_expr(pos);
            }
            Some(ScopeItem::Class(class_id)) => {
                if n.type_args.is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("`{}` is not generic", name),
                        ident_pos.clone(),
                    );
                }
                Some(class_id)
            }
            Some(ScopeItem::GenericClass(key)) => match &n.type_args {
                Some(type_args) => {
                    let resolved: Vec<Type> = type_args
                        .params
                        .iter()
                        .map(|t| self.resolve_type(t))
                        .collect();
                    self.instantiate_class(&key, &resolved, ident_pos.clone())
                }
                None => {
                    self.error(
                        RuleCode::S100,
                        format!("generic class `{}` requires explicit type arguments", name),
                        ident_pos.clone(),
                    );
                    None
                }
            },
            _ => {
                let code = if self.ambient_namespace(callee, fx).is_some() {
                    RuleCode::S100
                } else {
                    RuleCode::S016
                };
                self.error(code, format!("unknown class `{}`", name), ident_pos.clone());
                None
            }
        };
        let Some(class_id) = class_id else {
            return self.err_expr(pos);
        };
        if self.handle_classes.contains(&class_id) {
            self.error(
                RuleCode::S100,
                format!(
                    "opaque handle `{}` is obtained from the host, not constructed",
                    name
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        // compiler.md §108.4 rule 5: a `declare class` in a program file
        // has no constructor body and no positional store, so no argument
        // reaches a field. A mirror class keeps `new`: `lower_new` stores
        // every argument into the field at the same position, which is the
        // mirror constructor's contract. An instance of an ambient generic
        // template reaches this site through the template's status.
        if self.declared_classes.contains(&class_id) && !self.classes[class_id.0].is_boundary {
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "ambient class `{name}` is obtained from the host, not constructed, because \
                     a `declare class` has no constructor body"
                ),
                pos.clone(),
                Divergence::AmbientClassConstruction,
            );
            return self.err_expr(pos);
        }
        if self.classes[class_id.0].is_descriptor {
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "descriptor class `{name}` is constructed with an object literal, not `new`"
                ),
                pos.clone(),
                Divergence::DescriptorConstruction,
            );
            return self.err_expr(pos);
        }
        let params = self.class_sigs[class_id.0].ctor.clone().unwrap_or_default();
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        let args = self.check_args(&params, args_ast, fx, &pos, &name);
        for arg in &args {
            if self.is_capturing_value(arg, fx) {
                self.error(
                    RuleCode::S009,
                    "capturing lambdas may not escape into constructed objects",
                    arg.pos.clone(),
                );
            }
        }
        hir::Expr {
            kind: ExprKind::New {
                class: class_id,
                args,
            },
            ty: Type::Class(class_id),
            pos,
        }
    }
}
