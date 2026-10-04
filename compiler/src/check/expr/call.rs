//! Checks call expressions, method calls, argument lists, and `new`.

use crate::check::rejection::RejectionSite;
use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{
    source_name, static_member_symbol, Checker, ContainerSlot, FnCtx, ParamSig, ScopeItem,
};
use crate::diag::Pos;
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
        let active = self.active_call.replace(c.span);
        let checked = self.check_call_inner(c, ctx, fx, pos);
        self.active_call = active;
        checked
    }

    fn check_call_inner(
        &mut self,
        c: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let ast::Callee::Expr(callee) = &c.callee else {
            let site = match &c.callee {
                ast::Callee::Super(_)
                    if c.type_args.is_none()
                        && fx
                            .frames
                            .last()
                            .is_some_and(|frame| frame.super_call_available) =>
                {
                    RejectionSite::SuperConstructorCall
                }
                ast::Callee::Super(_) => RejectionSite::NonExpressionCalleeUnavailable,
                ast::Callee::Import(_) => {
                    let missing_target =
                        c.args
                            .first()
                            .is_some_and(|argument| match &*argument.expr {
                                ast::Expr::Lit(ast::Lit::Str(path)) => {
                                    !self.prog.files.iter().any(|file| {
                                        file.stem
                                            == crate::check::normalize_module_specifier(
                                                path.value.as_ref(),
                                            )
                                    })
                                }
                                ast::Expr::Lit(_) => true,
                                _ => false,
                            });
                    if c.type_args.is_some()
                        || c.args.is_empty()
                        || c.args.len() > 2
                        || missing_target
                    {
                        RejectionSite::NonExpressionCalleeUnavailable
                    } else {
                        RejectionSite::DynamicImportCall
                    }
                }
                ast::Callee::Expr(_) => RejectionSite::NonExpressionCalleeUnavailable,
            };
            self.reject_subset(site, "call form outside the decided surface", pos.clone());
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
        self.with_expression_work(|checker| {
            let name = id.sym.to_string();
            let ident_pos = checker.pos(id.span);
            if fx.owns_local_name(&name) {
                let callee = checker.check_ident(id, None, fx);
                return checker.check_indirect_call(callee, c, fx, pos);
            }
            let item = checker.scope_item(&name, &ident_pos);
            if item.is_none() && crate::check::exception::ErrorKind::from_name(&name).is_some() {
                return checker.reject_error_call(&name, pos);
            }
            if c.type_args.is_some()
                && matches!(item, Some(ScopeItem::Func(_)) | Some(ScopeItem::Foreign(_)))
            {
                checker.reject_subset(
                    RejectionSite::NonGenericFunctionTypeArguments,
                    format!("`{}` is not generic", name),
                    ident_pos.clone(),
                );
            }
            match item {
                Some(ScopeItem::Poisoned | ScopeItem::Namespace { .. }) => {
                    checker.check_poisoned_arguments(&c.args, fx);
                    checker.err_expr(pos)
                }
                Some(ScopeItem::Func(f)) => checker.check_direct_call(&f, c, fx, pos),
                Some(ScopeItem::Foreign(f)) => checker.check_foreign_call(&f, c, fx, pos),
                Some(ScopeItem::GenericFunc(key)) => {
                    let (arguments, checked) = if let Some(type_args) = &c.type_args {
                        (checker.resolve_instance_arguments(type_args), None)
                    } else {
                        let Some((arguments, checked)) =
                            checker.infer_call_arguments(&key, c, fx, &ident_pos)
                        else {
                            return checker.err_expr(pos);
                        };
                        (arguments, Some(checked))
                    };
                    match checker.instantiate_fn(&key, &arguments, ident_pos) {
                        Some(mono) => {
                            checker.check_direct_call_with_arguments(&mono, c, fx, pos, checked)
                        }
                        None => checker.err_expr(pos),
                    }
                }
                Some(ScopeItem::Global(_)) => {
                    let callee = checker.check_ident(id, None, fx);
                    checker.check_indirect_call(callee, c, fx, pos)
                }
                Some(ScopeItem::Class(_)) | Some(ScopeItem::GenericClass(_)) => {
                    checker.reject_subset(
                        RejectionSite::ClassCalledWithoutNew,
                        format!("`{}` is a class; construct it with `new`", name),
                        ident_pos.clone(),
                    );
                    checker.err_expr(pos)
                }
                Some(ScopeItem::Enum(_)) => {
                    checker.reject_subset(
                        RejectionSite::EnumCalled,
                        format!("enum `{}` is not callable", name),
                        ident_pos.clone(),
                    );
                    checker.err_expr(pos)
                }
                Some(ScopeItem::TypeAlias(_)) => {
                    checker.reject_subset(
                        RejectionSite::MirrorTypeAliasCalled,
                        format!("type alias `{name}` used as a value"),
                        ident_pos.clone(),
                    );
                    checker.err_expr(pos)
                }
                Some(ScopeItem::StringAlias(_)) => {
                    checker.reject_subset(
                        RejectionSite::LiteralAliasCalled,
                        format!("string-literal union alias `{name}` is not callable"),
                        ident_pos.clone(),
                    );
                    checker.err_expr(pos)
                }
                None => {
                    if let Some(function) = super::super::text::uri_function(&name) {
                        return checker.check_uri_call(function, c, fx, pos, &name);
                    }
                    if name == "eval" {
                        checker.reject_subset(
                            RejectionSite::DynamicEvaluatorCalled,
                            "no dynamic code evaluation",
                            pos.clone(),
                        );
                        return checker.err_expr(pos);
                    }
                    if let Some(f) = crate::ambient::number_global(&name) {
                        return checker.check_number_global_call(f, c, fx, pos, &name);
                    }
                    if name == "Number" {
                        checker.reject_api_form(
                            "Number",
                            "Number(value)",
                            "Number(value)",
                            pos.clone(),
                        );
                        return checker.err_expr(pos);
                    }
                    if name == "isNaN" || name == "isFinite" {
                        let surface = if name == "isNaN" {
                            "isNaN(value)"
                        } else {
                            "isFinite(value)"
                        };
                        checker.reject_api_form("global", surface, surface, pos.clone());
                        return checker.err_expr(pos);
                    }
                    if let Some(ambient) = crate::ambient::ambient_fn(&name) {
                        if ambient == AmbientFn::Unreachable && !unreachable_statement {
                            checker.reject_subset(
                                RejectionSite::UnreachableExpressionValue,
                                "`unreachable()` is only legal as a call statement",
                                pos.clone(),
                            );
                            let _ = checker.check_ambient_call(ambient, c, fx, pos.clone());
                            return checker.err_expr(pos);
                        }
                        return checker.check_ambient_call(ambient, c, fx, pos);
                    }
                    checker.reject_subset(
                        if crate::ambient::lib_callable_name(&name) {
                            RejectionSite::UnknownFunctionName
                        } else {
                            RejectionSite::UnboundFunctionName
                        },
                        format!("unknown function `{}`", name),
                        ident_pos,
                    );
                    checker.err_expr(pos)
                }
            }
        })
    }

    fn check_direct_call(
        &mut self,
        fn_name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        self.check_direct_call_with_arguments(fn_name, c, fx, pos, None)
    }

    fn check_direct_call_with_arguments(
        &mut self,
        fn_name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        checked: Option<Vec<Option<hir::Expr>>>,
    ) -> hir::Expr {
        if !self.defers_call_arguments(self.fn_sigs.get(fn_name).is_some_and(|sig| sig.generic)) {
            self.decide_function_parameters(fn_name);
        }
        let Some(sig) = self.fn_sigs.get(fn_name).cloned() else {
            return self.err_expr(pos);
        };
        if sig.is_async {
            let args = self.check_args_with_arguments(
                RejectionSite::SourceFunctionArgumentCount,
                &sig.params,
                &c.args,
                fx,
                &pos,
                fn_name,
                checked,
                sig.generic,
            );
            let origin = fx.register_async_origin(pos.clone());
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::AsyncHandleCreate {
                    callee: AsyncCallee::Function(hir::Symbol::from_full_text(fn_name)),
                    args,
                    origin,
                },
                ty: Type::async_handle(sig.ret),
                pos,
            };
        }
        if sig.is_generator && !sig.yield_known {
            self.reject_subset(
                RejectionSite::GeneratorYieldTypeNotKnown,
                format!(
                    "generator `{}` is called before its yield type is known; \
                     declare it earlier in the program",
                    source_name(fn_name)
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let args = self.check_args_with_arguments(
            RejectionSite::ForeignFunctionArgumentCount,
            &sig.params,
            &c.args,
            fx,
            &pos,
            fn_name,
            checked,
            sig.generic,
        );
        let value = hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Func(hir::Symbol::from_full_text(fn_name)),
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
        let args = self.check_args(
            RejectionSite::ContextMethodArgumentCount,
            &sig.params,
            &c.args,
            fx,
            &pos,
            name,
        );
        hir::Expr {
            pending_work: None,
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
        let args = self.check_args(
            RejectionSite::AmbientFunctionArgumentCount,
            &params,
            &c.args,
            fx,
            &pos,
            &label,
        );
        hir::Expr {
            pending_work: None,
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
        if self.apparent_type(ty).is_numeric()
            || matches!(
                &self.apparent_type(ty),
                Type::Bool | Type::Enum(_) | Type::Error
            )
        {
            return None;
        }
        match &self.apparent_type(ty) {
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
            self.reject_subset(
                RejectionSite::ContextBytesMissingType,
                format!("`Context.{name}<T>` takes exactly one type argument"),
                member_pos,
            );
            return self.err_expr(pos);
        };
        if type_args.params.len() != 1 {
            self.reject_subset(
                RejectionSite::ContextBytesTypeCount,
                format!("`Context.{name}<T>` takes exactly one type argument"),
                member_pos,
            );
            return self.err_expr(pos);
        }
        let target = self.resolve_type(&type_args.params[0]);
        if self.apparent_type(&(target)) == Type::Error {
            return self.err_expr(pos);
        }
        if !self.instance_restriction(
            crate::check::opaque::InstanceRestriction::ByteAccessTarget,
            &target,
        ) {
            let top_level_ok = match &self.apparent_type(&target) {
                Type::FixedArray(..) => true,
                Type::Class(id) => self.classes.get(id.0).is_some_and(|class| class.is_value),
                _ => false,
            };
            if !top_level_ok {
                let target_name = self.type_name(&target);
                self.reject_subset(RejectionSite::ContextByteTargetKind, format!(
                    "`Context.{name}<T>` cannot use `{target_name}`; it is not a @ValueType value class or FixedArray"
                ), member_pos);
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
                self.reject_subset(
                    RejectionSite::ContextByteTargetLayout,
                    format!("`Context.{name}<T>` cannot use `{target_name}`; {detail}"),
                    member_pos,
                );
                return self.err_expr(pos);
            }
        }

        let params = match function {
            ContextBytesFn::BytesOf => vec![target.clone()],
            ContextBytesFn::BytesInto => {
                vec![target.clone(), Type::array(Type::U8), Type::U32]
            }
            ContextBytesFn::FromBytes => {
                vec![Type::array(Type::U8), Type::U32]
            }
        };
        if call.args.len() != params.len() {
            self.reject_subset(
                RejectionSite::ContextBytesArgumentCount,
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
                self.reject_subset(
                    RejectionSite::ContextBytesSpread,
                    "spread arguments require variadic parameters, which the language does not have",
                    spread_pos.clone(),
                );
                return self.err_expr(spread_pos);
            }
            let checked = self.check_expr(&argument.expr, Some(expected), fx);
            if self.involves_type_parameter(expected) || self.involves_type_parameter(&checked.ty) {
                self.require_expr_assignable(
                    &checked,
                    expected,
                    fx,
                    &format!("`Context.{name}` argument"),
                );
            } else if checked.ty != *expected && self.apparent_type(&(checked.ty)) != Type::Error {
                self.reject_subset(
                    if matches!(
                        (
                            &self.apparent_type(&checked.ty),
                            &self.apparent_type(expected)
                        ),
                        (Type::FixedArray(..), Type::FixedArray(..))
                            | (Type::Class(_), Type::Class(_))
                    ) {
                        RejectionSite::ByteArgumentIdentity
                    } else {
                        RejectionSite::ByteArgumentTypeMismatch
                    },
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
            ContextBytesFn::BytesOf => Type::array(Type::U8),
            ContextBytesFn::BytesInto => Type::Void,
            ContextBytesFn::FromBytes => target.clone(),
        };
        hir::Expr {
            pending_work: None,
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
        self.check_indirect_call_with_arguments(callee, c, fx, pos, None)
    }

    fn check_indirect_call_with_arguments(
        &mut self,
        callee: hir::Expr,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        checked_arguments: Option<Vec<Option<hir::Expr>>>,
    ) -> hir::Expr {
        self.with_expression_work(|checker| {
            if let Type::GenericUnion(members) = &callee.ty {
                let members = members.clone();
                let mut result: Option<hir::Expr> = None;
                let mut arguments = checked_arguments;
                for member in members.iter() {
                    let mut value = callee.clone();
                    value.ty = member.clone();
                    let checked = checker.check_indirect_call_with_arguments(
                        value,
                        c,
                        fx,
                        pos.clone(),
                        arguments.clone(),
                    );
                    if arguments.is_none() {
                        if let ExprKind::Call { args, .. } = &checked.kind {
                            arguments = Some(args.iter().cloned().map(Some).collect());
                        }
                    }
                    if let Some(result) = &mut result {
                        result.ty = checker.generic_union(&result.ty, &checked.ty);
                    } else {
                        result = Some(checked);
                    }
                }
                return result.unwrap_or_else(|| checker.err_expr(pos));
            }
            let callee = checker.apparent_expr(callee);
            match checker.apparent_type(&callee.ty.clone()) {
                Type::Func(ft) => {
                    let params: Vec<ParamSig> = ft
                        .params
                        .iter()
                        .map(|t| ParamSig::positional(t.clone()))
                        .collect();
                    let args = checker.check_args_with_arguments(
                        RejectionSite::FunctionValueArgumentCount,
                        &params,
                        &c.args,
                        fx,
                        &pos,
                        "the function value",
                        checked_arguments,
                        false,
                    );
                    let value = hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Call {
                            callee: Callee::Value(Box::new(callee)),
                            args,
                        },
                        ty: ft.ret.clone(),
                        pos,
                    };
                    checker.track_async_call_result(value, fx)
                }
                Type::Error => checker.err_expr(pos),
                other => {
                    let name = checker.type_name(&other);
                    checker.nullable_use_error(
                        &callee,
                        fx,
                        (
                            RejectionSite::NullableCall,
                            RejectionSite::NullableCallShared,
                        ),
                        format!("type `{}` is not callable", name),
                        pos.clone(),
                    );
                    checker.err_expr(pos)
                }
            }
        })
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
        match self.peek_scope_item(receiver.sym.as_ref()) {
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
        let receiver_pos = self.pos(receiver.span);
        let Some(ScopeItem::Class(class)) = self.scope_item(&class_name, &receiver_pos) else {
            return None;
        };
        if self.reject_member_access(class, name, true, false, fx, member_pos.clone()) {
            return Some(self.err_expr(pos));
        }
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
            let symbol = static_member_symbol(class, &self.classes[class.0].name, &instance);
            return Some(self.check_direct_call(&symbol, call, fx, pos));
        }
        if !self.class_sigs[class.0].static_methods.contains_key(name) {
            return None;
        }
        if call.type_args.is_some() {
            self.reject_subset(
                RejectionSite::NonGenericStaticMethodTypeArguments,
                format!("static method `{class_name}.{name}` is not generic"),
                member_pos,
            );
        }
        let symbol = static_member_symbol(class, &self.classes[class.0].name, name);
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
        if self.reject_static_this_member(m, fx) {
            return self.err_expr(pos);
        }
        let ast::MemberProp::Ident(prop) = &m.prop else {
            let value = self.check_member_read(m, fx);
            return self.check_indirect_call(value, c, fx, pos);
        };
        if fx.field_initializer.is_some()
            && matches!(super::unparen_expr(&m.obj), ast::Expr::This(_))
        {
            let value = self.check_member_read(m, fx);
            return self.check_indirect_call(value, c, fx, pos);
        }
        let name = prop.sym.to_string();
        let prop_pos = self.pos(prop.span);
        if matches!(name.as_str(), "then" | "catch" | "finally") {
            self.reject_subset(
                RejectionSite::PromiseCombinatorCall,
                format!("Promise combinator `.{name}(...)` is not in the language"),
                prop_pos.clone(),
            );
            return self.err_expr(pos);
        }
        if self.ambient_namespace(&m.obj, fx) == Some("Promise") {
            self.reject_subset(
                RejectionSite::PromiseStaticCall,
                format!("Promise static `Promise.{name}(...)` is not in the language"),
                prop_pos.clone(),
            );
            return self.err_expr(pos);
        }
        if matches!(&*m.obj, ast::Expr::Ident(id) if id.sym.as_ref() == "Worker")
            && self.worker_is_ambient(fx)
        {
            if name == "spawn" {
                return self.check_worker_spawn(c, fx, pos);
            }
            self.reject_subset(
                if super::is_object_member(&name) {
                    RejectionSite::WorkerStaticObjectMethod
                } else {
                    RejectionSite::WorkerStaticUnknownMethod
                },
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
        // Resolves Array namespace calls (stdlib.md §9.11; compiler.md §105).
        if self.ambient_namespace(&m.obj, fx) == Some("Array") {
            return self.check_array_static_call(&name, c, ctx, fx, pos, prop_pos);
        }
        if let Some(handled) =
            self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx, false)
        {
            // Enum members are values, not callables.
            if matches!(self.apparent_type(&handled.ty), Type::Error) {
                return handled;
            }
            return self.check_indirect_call(handled, c, fx, pos);
        }
        let recv = self.check_receiver(&m.obj, fx);
        self.check_method_call_on(recv, prop, c, fx, pos)
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
            self.reject_subset(
                RejectionSite::GenericMethodTypeArgumentsMissing,
                format!("generic method `{name}` requires explicit type arguments"),
                pos,
            );
            return None;
        };
        let arguments = self.resolve_instance_arguments(type_args);
        self.instantiate_method(class, name, &arguments, is_static, pos)
    }

    /// Checks `receiver.name(...)` from an already-checked receiver.
    pub(in crate::check) fn check_method_call_on(
        &mut self,
        recv: hir::Expr,
        property: &ast::IdentName,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        self.with_expression_work(|checker| {

        if let Type::GenericUnion(members) = &recv.ty {
            let members = members.clone();
            let mut result: Option<hir::Expr> = None;
            for member in members.iter() {
                let mut value = recv.clone();
                value.ty = member.clone();
                let checked = checker.check_method_call_on(value, property, c, fx, pos.clone());
                if let Some(result) = &mut result {
                    result.ty = checker.generic_union(&result.ty, &checked.ty);
                } else {
                    result = Some(checked);
                }
            }
            return result.unwrap_or_else(|| checker.err_expr(pos));
        }
        let recv = checker.apparent_expr(recv);
        let mut name = property.sym.to_string();
        let prop_pos = checker.pos(property.span);
        // §143 rule 1a: a method call on a type parameter with
        // no constraint is an error for every type argument (`tsc` TS2339).
        if checker.is_unconstrained_type_parameter(&recv.ty) {
            let type_name = checker.type_name(&recv.ty);
            checker.reject_subset(
                RejectionSite::ValueClassMethodMissing,
                format!("`{type_name}` has no method `{name}`"),
                prop_pos,
            );
            checker.check_poisoned_arguments(&c.args, fx);
            return checker.err_expr(pos);
        }
        // §82.4 rule 3: the call names the instance, not the template.
        if let Type::Class(class) = &checker.apparent_type(&recv.ty) {
            let class = *class;
            if checker.reject_member_access(class, &name, false, false, fx, prop_pos.clone()) {
                return checker.err_expr(pos);
            }
            if checker.class_sigs[class.0].has_generic_method(&name, false) {
                let Some(instance) =
                    checker.instantiate_generic_method_call(class, &name, c, false, prop_pos.clone())
                else {
                    return checker.err_expr(pos);
                };
                name = instance;
            }
        }
        if checker.is_error_type(&recv.ty) && name == "toString" {
            checker.check_args(
                RejectionSite::ScalarToStringArgumentCount,
                &[],
                &c.args,
                fx,
                &pos,
                &name,
            );
            if c.type_args.is_some() {
                checker.reject_subset(
                    RejectionSite::ToStringTypeArguments,
                    "`toString` is not generic",
                    pos.clone(),
                );
            }
            return checker.error_to_string(recv, pos);
        }
        let name = name;
        let mk = |recv: hir::Expr, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Method {
                    recv: Box::new(recv),
                    name: hir::Symbol::from_full_text(name.clone()),
                },
                args,
            },
            ty,
            pos,
        };
        match checker.apparent_type(&recv.ty.clone()) {
            Type::Error => checker.err_expr(pos),
            ty if checker.apparent_type(&ty).is_numeric() => {
                checker.check_number_method(recv, &name, c, fx, pos, prop_pos)
            }
            Type::Date => checker.check_date_method(recv, &name, c, fx, pos, prop_pos),
            Type::Map(key, value) => {
                checker.check_map_method(recv, *key, *value, &name, c, fx, pos, prop_pos)
            }
            Type::Set(key) => checker.check_set_method(recv, *key, &name, c, fx, pos, prop_pos),
            Type::Worker(input, output) => {
                let (function, params, ret) = match name.as_str() {
                    "post" => (
                        WorkerFn::Post,
                        vec![ParamSig {
                            name: "message".to_string(),
                            state: crate::check::initializer::TypeState::decided((*input).clone()),
                            initializer: None,
                            has_default: false,
                        }],
                        Type::Void,
                    ),
                    "poll" => (WorkerFn::Poll, Vec::new(), Type::Nullable(output.clone())),
                    "close" => (WorkerFn::Close, Vec::new(), Type::Void),
                    "join" => (WorkerFn::Join, Vec::new(), Type::Void),
                    _ => {
                        let type_name = checker.type_name(&Type::Worker(input, output));
                        checker.reject_subset(
                            if super::is_object_member(&name) {
                                RejectionSite::WorkerObjectMethod
                            } else {
                                RejectionSite::WorkerUnknownMethod
                            },
                            format!("`{type_name}` has no method `{name}`"),
                            prop_pos,
                        );
                        return checker.err_expr(pos);
                    }
                };
                let mut args = vec![recv];
                args.extend(checker.check_args(
                    RejectionSite::InboxWaitArgumentCount,
                    &params,
                    &c.args,
                    fx,
                    &pos,
                    &name,
                ));
                hir::Expr {
                    pending_work: None,
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
                        let type_name = checker.type_name(&Type::Inbox(message));
                        checker.reject_subset(
                            if super::is_object_member(&name) {
                                RejectionSite::InboxObjectMethod
                            } else {
                                RejectionSite::InboxUnknownMethod
                            },
                            format!("`{type_name}` has no method `{name}`"),
                            prop_pos,
                        );
                        return checker.err_expr(pos);
                    }
                };
                let mut args = vec![recv];
                args.extend(checker.check_args(
                    RejectionSite::OutboxPostArgumentCount,
                    &[],
                    &c.args,
                    fx,
                    &pos,
                    &name,
                ));
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Call {
                        callee: Callee::Worker(function),
                        args,
                    },
                    ty: Type::nullable(*message),
                    pos,
                }
            }
            Type::Outbox(message) => {
                if name != "post" {
                    let type_name = checker.type_name(&Type::Outbox(message));
                    checker.reject_subset(
                        if super::is_object_member(&name) {
                            RejectionSite::OutboxObjectMethod
                        } else {
                            RejectionSite::OutboxUnknownMethod
                        },
                        format!("`{type_name}` has no method `{name}`"),
                        prop_pos,
                    );
                    return checker.err_expr(pos);
                }
                let params = [ParamSig {
                    name: "message".to_string(),
                    state: crate::check::initializer::TypeState::decided((*message).clone()),
                    initializer: None,
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(checker.check_args(
                    RejectionSite::FixedArrayPushArgumentCount,
                    &params,
                    &c.args,
                    fx,
                    &pos,
                    &name,
                ));
                hir::Expr {
                    pending_work: None,
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
                    let args = checker.check_args(
                        RejectionSite::ArrayPushArgumentCount,
                        &params,
                        &c.args,
                        fx,
                        &pos,
                        "push",
                    );
                    mk(recv, args, Type::I32, pos)
                }
                "pop" => {
                    let args = checker.check_args(
                        RejectionSite::ArrayPopArgumentCount,
                        &[],
                        &c.args,
                        fx,
                        &pos,
                        "pop",
                    );
                    mk(recv, args, (*elem).clone(), pos)
                }
                other => {
                    // The §9 method intrinsics (stdlib.md §9, Q22).
                    if other == "toString" && !c.args.is_empty() {
                        checker.reject_subset(
                            RejectionSite::ToStringArgumentCount,
                            "`toString` expects no arguments",
                            pos.clone(),
                        );
                        return checker.err_expr(pos);
                    }
                    if let Some(f) = crate::ambient::arr_method(other) {
                        return checker.check_array_method(
                            recv,
                            (*elem).clone(),
                            (f, &name),
                            c,
                            fx,
                            pos,
                        );
                    }
                    if !checker.arr_subset_rejection(other, prop_pos.clone()) {
                        checker.arr_surface_error(other, prop_pos.clone());
                    }
                    checker.err_expr(pos)
                }
            },
            // Q27 accepts the closure-taking `every` family on the
            // in-place fixed buffer. Other checker-owned Array methods
            // retain a named S014; `push`/`pop` are not in that table and
            // keep the standing "no method" diagnostic.
            Type::FixedArray(elem, n) => {
                if let Some(f) = crate::ambient::arr_method(&name) {
                    if f.fixed_symbol().is_some() {
                        return checker.check_array_method(
                            recv,
                            (*elem).clone(),
                            (f, &name),
                            c,
                            fx,
                            pos,
                        );
                    }
                    checker.reject_api_form(
                        "FixedArray<T, N>",
                        "non-callback T[] methods",
                        &name,
                        prop_pos.clone(),
                    );
                    return checker.err_expr(pos);
                }
                let type_name = checker.type_name(&Type::FixedArray(elem, n));
                checker.reject_subset(
                    if super::is_object_member(&name) {
                        RejectionSite::FixedArrayObjectMethod
                    } else {
                        RejectionSite::FixedArrayUnknownMethod
                    },
                    format!("`{type_name}` has no method `{name}`"),
                    prop_pos,
                );
                checker.err_expr(pos)
            }
            Type::Str => match name.as_str() {
                "search" | "replace" | "replaceAll" | "split" => {
                    checker.check_string_pattern_method(recv, &name, c, fx, pos, prop_pos)
                }
                other => {
                    // The §8 method intrinsics (stdlib.md §8, Q21).
                    if let Some(f) = crate::ambient::str_method(other) {
                        return checker.check_str_method(recv, (f, &name), c, fx, pos);
                    }
                    if !checker.str_subset_rejection(other, prop_pos.clone()) {
                        checker.str_surface_error(other, prop_pos.clone());
                    }
                    checker.err_expr(pos)
                }
            },
            Type::RegExp => checker.check_regex_method(recv, &name, c, fx, pos, prop_pos),
            Type::Generator(y) => match name.as_str() {
                "next" => {
                    let args = checker.check_args(
                        RejectionSite::GeneratorNextArgumentCount,
                        &[],
                        &c.args,
                        fx,
                        &pos,
                        "next",
                    );
                    let step = Type::iter_result((*y).clone());
                    match crate::check::layout::class_independent_layout(&step) {
                        crate::check::layout::IndependentLayout::Fits => mk(recv, args, step, pos),
                        crate::check::layout::IndependentLayout::TooLarge => {
                            checker.reject_subset(
                                RejectionSite::CoroutineStepLayoutLimit,
                                format!(
                                    "coroutine step-result layout exceeds the supported \
                                     aggregate limit of {} bytes",
                                    crate::types::MAX_AGGREGATE_BYTES
                                ),
                                prop_pos,
                            );
                            checker.err_expr(pos)
                        }
                        crate::check::layout::IndependentLayout::DependsOnClass => {
                            checker.pending_layouts.push((
                                step.clone(),
                                prop_pos,
                                "coroutine step-result layout",
                            ));
                            mk(recv, args, step, pos)
                        }
                    }
                }
                other => {
                    checker.reject_subset(
                        RejectionSite::CoroutineReturnOrThrowCall,
                        format!("`{}` is outside the coroutine surface (next)", other),
                        prop_pos.clone(),
                    );
                    checker.err_expr(pos)
                }
            },
            Type::Class(id) => {
                if checker.classes[id.0].fields.iter().any(|field| {
                    field.name == name
                        && (checker.apparent_type(&field.ty) == Type::Error
                            || checker.apparent_type(&field.ty).function_type().is_some())
                }) {
                    let mut field = checker.member_on(recv, &name, prop_pos, None, fx);
                    checker.apply_narrowing(&mut field, fx);
                    return checker.check_indirect_call(field, c, fx, pos);
                }
                let generic = checker.class_sigs[id.0].methods.get(&name).is_some_and(|sig| sig.generic);
                if !checker.defers_call_arguments(generic) {
                    checker.decide_method_parameters(id, &name, false);
                }
                let sig = checker.class_sigs[id.0].methods.get(&name).cloned();
                match sig {
                    Some(sig) => {
                        if sig.is_async {
                            let args = checker.check_args_with_arguments(
                                RejectionSite::AsyncMethodArgumentCount,
                                &sig.params,
                                &c.args,
                                fx,
                                &pos,
                                &name,
                                                        None,
                            sig.generic,
                            );
                            let origin = fx.register_async_origin(pos.clone());
                            return hir::Expr {
                                pending_work: None,
                                kind: ExprKind::AsyncHandleCreate {
                                    callee: AsyncCallee::Method {
                                        class: id,
                                        receiver: Box::new(recv),
                                        name: hir::Symbol::from_full_text(name),
                                    },
                                    args,
                                    origin,
                                },
                                ty: Type::async_handle(sig.ret),
                                pos,
                            };
                        }
                        let args = checker.check_args_with_arguments(
                            RejectionSite::InstanceMethodArgumentCount,
                            &sig.params,
                            &c.args,
                            fx,
                            &pos,
                            &name,
                                                None,
                        sig.generic,
                        );
                        let value = mk(recv, args, sig.ret, pos);
                        checker.track_async_call_result(value, fx)
                    }
                    None => {
                        let class_name = checker.classes[id.0].name.clone();
                        if checker.class_sigs[id.0].has_static_member(&name) {
                            checker.reject_subset(RejectionSite::InstanceStaticMethodCall, format!(
                                    "`{class_name}.{name}` is static and must be accessed through the class name"
                                ), prop_pos.clone());
                            return checker.err_expr(pos);
                        }
                        checker.reject_subset(
                            if super::is_object_member(&name) {
                                RejectionSite::ClassObjectMethodCall
                            } else {
                                RejectionSite::ClassUndeclaredMethodCall
                            },
                            format!("`{}` has no method `{}`", class_name, name),
                            prop_pos.clone(),
                        );
                        checker.err_expr(pos)
                    }
                }
            }
            other => {
                let type_name = checker.type_name(&other);
                checker.reject_subset(
                    match checker.apparent_type(&other) {
                        Type::Bool => RejectionSite::BooleanMethod,
                        Type::Func(_) => RejectionSite::FunctionMethod,
                        Type::Generator(_) => RejectionSite::GeneratorMethod,
                        Type::Enum(_) => RejectionSite::EnumMethod,
                        Type::StringAlias(_) => RejectionSite::LiteralAliasMethod,
                        _ => RejectionSite::InvalidReceiverMethod,
                    },
                    format!("`{}` has no method `{}`", type_name, name),
                    prop_pos.clone(),
                );
                checker.err_expr(pos)
            }
        }

        })
    }

    pub(super) fn check_poisoned_arguments(&mut self, args: &[ast::ExprOrSpread], fx: &mut FnCtx) {
        for argument in args {
            let _ = self.check_expr(&argument.expr, None, fx);
        }
    }

    /// Reports an argument count outside `(total, required)` for the
    /// callee `what`. A spread argument gives no count.
    fn check_argument_count(
        &mut self,
        site: RejectionSite,
        (total, required): (usize, usize),
        args: &[ast::ExprOrSpread],
        pos: &Pos,
        what: &str,
    ) {
        let has_spread = args.iter().any(|arg| arg.spread.is_some());
        if !has_spread && (args.len() < required || args.len() > total) {
            self.reject_subset(
                site,
                format!(
                    "`{}` expects {} argument(s) ({} required), got {}",
                    source_name(what),
                    total,
                    required,
                    args.len()
                ),
                pos.clone(),
            );
        }
    }

    pub(in crate::check) fn check_args(
        &mut self,
        site: RejectionSite,
        params: &[ParamSig],
        args: &[ast::ExprOrSpread],
        fx: &mut FnCtx,
        pos: &Pos,
        what: &str,
    ) -> Vec<hir::Expr> {
        self.check_args_with_arguments(site, params, args, fx, pos, what, None, false)
    }

    pub(in crate::check) fn check_args_with_arguments(
        &mut self,
        site: RejectionSite,
        params: &[ParamSig],
        args: &[ast::ExprOrSpread],
        fx: &mut FnCtx,
        pos: &Pos,
        what: &str,
        checked: Option<Vec<Option<hir::Expr>>>,
        generic_callee: bool,
    ) -> Vec<hir::Expr> {
        if self.defers_call_arguments(generic_callee)
            && matches!(
                site,
                RejectionSite::ForeignFunctionArgumentCount
                    | RejectionSite::SourceFunctionArgumentCount
                    | RejectionSite::InstanceMethodArgumentCount
                    | RejectionSite::AsyncMethodArgumentCount
            )
        {
            let checked = checked.unwrap_or_else(|| vec![None; args.len()]);
            let slots = args
                .iter()
                .zip(&checked)
                .map(|(argument, value)| {
                    value
                        .clone()
                        .unwrap_or_else(|| self.err_expr(self.pos(argument.expr.span())))
                })
                .collect();
            self.defer_work(
                crate::check::initializer::DeferredWork::Arguments {
                    source: args.to_vec(),
                    params: params.to_vec(),
                    site,
                    what: what.to_string(),
                    checked: Some(checked),
                },
                fx,
            );
            return slots;
        }
        let mut checked = checked.unwrap_or_default().into_iter();
        let required = params.iter().filter(|p| !p.has_default).count();
        if params
            .iter()
            .all(|parameter| self.apparent_type(parameter.ty()) != Type::Error)
        {
            self.check_argument_count(site, (params.len(), required), args, pos, what);
        }
        let mut out = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let prechecked = checked.next().flatten();
            if arg.spread.is_some() {
                let p = self.pos(arg.spread.unwrap_or_default());
                self.reject_subset(
                    RejectionSite::CallSpread,
                    "spread arguments require variadic parameters, which the language does not have",
                    p,
                );
                continue;
            }
            if params.get(i).is_some_and(|p| {
                matches!(p.state, crate::check::initializer::TypeState::InProgress)
            }) {
                self.cycle(self.pos(arg.expr.span()));
            }
            let param_ty = params.get(i).map(|p| p.ty().clone());
            let callback_context =
                generic_callee && matches!(super::unparen_expr(&arg.expr), ast::Expr::Arrow(_));
            let saved_context =
                std::mem::replace(&mut self.generic_callback_context, callback_context);
            let checked =
                prechecked.unwrap_or_else(|| self.check_expr(&arg.expr, param_ty.as_ref(), fx));
            self.generic_callback_context = saved_context;
            if let Some(param_ty) = param_ty {
                self.require_expr_assignable(&checked, &param_ty, fx, "the argument");
                if matches!(
                    &self.apparent_type(&param_ty),
                    Type::AsyncHandle(_) | Type::Array(_)
                ) {
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
            self.reject_subset(
                RejectionSite::SetSourceSpread,
                "spread arguments require variadic parameters, which the language does not have",
                spread_pos,
            );
            return None;
        }
        let source = self.check_expr(&argument.expr, None, fx);
        let element = match &self.apparent_type(&source.ty) {
            Type::Error => return None,
            // A Map source yields pairs, which need a tuple type.
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
            other => match self.apparent_type(other).iteration_element() {
                Some((_, element)) => element,
                None => {
                    let actual = self.type_name(other);
                    self.reject_subset(
                        RejectionSite::SetSourceDomain,
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

    /// Checks a `new` expression. `ctx` is the contextual type; a
    /// container construction reads the poisoned arguments of it
    /// (compiler.md §132 rule 2).
    pub(super) fn check_new(
        &mut self,
        n: &ast::NewExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let mut callee: &ast::Expr = &n.callee;
        while let ast::Expr::Paren(p) = callee {
            callee = &p.expr;
        }
        let ast::Expr::Ident(id) = callee else {
            self.reject_subset(
                RejectionSite::ConstructorNotNamedClass,
                "`new` requires a class name",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let name = id.sym.to_string();
        let ident_pos = self.pos(id.span);
        if fx.owns_local_name(&name) {
            if self
                .lookup_local(&name, &ident_pos, fx)
                .is_some_and(|local| !matches!(self.apparent_type(&local.ty), Type::Error))
            {
                self.reject_subset(
                    RejectionSite::LocalValueConstructed,
                    format!("`{name}` names a local value here, not a class"),
                    ident_pos.clone(),
                );
            }
            return self.err_expr(pos);
        }
        if name == "Function" {
            self.reject_subset(
                RejectionSite::DynamicFunctionConstructed,
                "no dynamic code evaluation (`new Function`)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "Promise" {
            self.reject_subset(RejectionSite::PromiseConstructed, "Promise objects cannot be constructed; async functions expose no Promise object surface", pos.clone());
            return self.err_expr(pos);
        }
        if matches!(name.as_str(), "Worker" | "Inbox" | "Outbox")
            && self.peek_scope_item(&name).is_none()
        {
            self.reject_subset(
                RejectionSite::WorkerEndpointConstructed,
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
        if self.error_name_is_ambient(&name, fx) {
            return self.check_error_new(&name, n, fx, pos);
        }
        // `new Date(ms)` (stdlib.md §3): the ambient constructor applies
        // only when neither a program declaration nor a function-local
        // binding shadows the name (same resolution as member access).
        if name == "Date" && self.date_is_ambient(fx) {
            return self.check_date_new(n, fx, pos);
        }
        if (name == "Map" || name == "Set") && self.assoc_is_ambient(&name, fx) {
            if name == "Map" && n.args.as_ref().is_some_and(|args| !args.is_empty()) {
                return self.check_map_copy(n, ctx, fx, pos, ident_pos);
            }
            let Some(type_args) = &n.type_args else {
                self.reject_subset(
                    RejectionSite::ContainerConstructorTypeArgumentsMissing,
                    format!("`new {name}` requires explicit type arguments (Q24)"),
                    ident_pos.clone(),
                );
                return self.err_expr(pos);
            };
            let expected = if name == "Map" { 2 } else { 1 };
            if type_args.params.len() != expected {
                self.reject_subset(
                    RejectionSite::ContainerConstructorTypeArgumentCount,
                    format!("`new {name}` takes exactly {expected} type argument(s)"),
                    ident_pos.clone(),
                );
                return self.err_expr(pos);
            }
            let arguments: &[ast::ExprOrSpread] = n.args.as_deref().unwrap_or(&[]);
            let saved_context = self.enter_container_context(ctx);
            let saved = self.in_assoc_key;
            self.in_assoc_key = true;
            let key = self.resolve_type(&type_args.params[0]);
            self.in_assoc_key = saved;
            let key_slot = if name == "Map" {
                ContainerSlot::MapKey
            } else {
                ContainerSlot::SetElement
            };
            let key_pos = self.pos(type_args.params[0].span());
            let key = self.container_argument(key_slot, key, key_pos);
            if !self.instance_restriction(
                crate::check::opaque::InstanceRestriction::AssociativeKey,
                &key,
            ) && !matches!(self.apparent_type(&key), Type::Error)
                && self.assoc_key_kind(&key).is_none()
            {
                let key_pos = self.pos(type_args.params[0].span());
                let key_name = self.type_name(&key);
                self.reject_subset(
                    RejectionSite::NewMapSetKey,
                    format!("`{key_name}` is not a permitted Map/Set key kind (Q24)"),
                    key_pos,
                );
            }
            if name == "Map" {
                let value = self.resolve_type(&type_args.params[1]);
                let value_pos = self.pos(type_args.params[1].span());
                let value = self.container_argument(ContainerSlot::MapValue, value, value_pos);
                self.leave_container_context(saved_context);
                let ty = Type::map(key, value);
                return hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Call {
                        callee: Callee::Map(MapFn::New),
                        args: Vec::new(),
                    },
                    ty,
                    pos,
                };
            }
            self.leave_container_context(saved_context);
            let ty = Type::set(key.clone());
            // The arity guard and the source index are one match, so no
            // caller holds an emptiness precondition for the other.
            let args = match arguments {
                [] => Vec::new(),
                [argument] => match self.check_set_source(argument, &key, fx) {
                    Some(source) => vec![source],
                    None => return self.err_expr(pos),
                },
                _ => {
                    self.reject_subset(
                        RejectionSite::SetConstructorSourceCount,
                        "`new Set` takes at most one source argument",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
            };
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: Callee::Set(SetFn::New),
                    args,
                },
                ty,
                pos,
            };
        }
        let class_id = match self.scope_item(&name, &ident_pos) {
            Some(ScopeItem::Poisoned | ScopeItem::Namespace { .. }) => {
                if let Some(arguments) = &n.args {
                    self.check_poisoned_arguments(arguments, fx);
                }
                return self.err_expr(pos);
            }
            Some(ScopeItem::Class(class_id)) => {
                if n.type_args.is_some() {
                    self.reject_subset(
                        RejectionSite::NonGenericConstructorTypeArguments,
                        format!("`{}` is not generic", name),
                        ident_pos.clone(),
                    );
                }
                Some(class_id)
            }
            Some(ScopeItem::GenericClass(key)) => match &n.type_args {
                Some(type_args) => {
                    let arguments = self.resolve_instance_arguments(type_args);
                    if arguments.types.contains(&Type::Error) {
                        // compiler.md §132 rule 2a: an error type argument
                        // gives no instance. The type-argument count and
                        // the constructor argument count do not depend on
                        // the argument types, so each still reports.
                        let _ = self.instantiate_class(&key, &arguments, ident_pos.clone());
                        let constructor_arguments: &[ast::ExprOrSpread] =
                            n.args.as_deref().unwrap_or(&[]);
                        let type_argument_count = self
                            .generic_classes
                            .get(&key)
                            .map(|template| template.type_params.len());
                        if type_argument_count == Some(arguments.types.len()) {
                            if let Some(arity) = self.template_constructor_arity(&key) {
                                self.check_argument_count(
                                    RejectionSite::ValueConstructorArgumentCount,
                                    arity,
                                    constructor_arguments,
                                    &pos,
                                    &name,
                                );
                            }
                        }
                        self.check_poisoned_arguments(constructor_arguments, fx);
                        return self.err_expr(pos);
                    }
                    self.instantiate_class(&key, &arguments, ident_pos.clone())
                }
                None => {
                    self.reject_subset(
                        RejectionSite::GenericConstructorTypeArguments,
                        format!("generic class `{}` requires explicit type arguments", name),
                        ident_pos.clone(),
                    );
                    None
                }
            },
            _ => {
                let site = if self.ambient_namespace(callee, fx).is_some()
                    || (self.type_scope_item(&name).is_none()
                        && crate::ambient::lib_value_name(&name))
                {
                    RejectionSite::UnknownNamespaceConstructor
                } else {
                    RejectionSite::UnknownClassConstructor
                };
                self.reject_subset(site, format!("unknown class `{}`", name), ident_pos.clone());
                None
            }
        };
        let Some(class_id) = class_id else {
            return self.err_expr(pos);
        };
        if self.reject_construction_modifier(class_id, fx, pos.clone()) {
            return self.err_expr(pos);
        }
        if self.handle_classes.contains(&class_id) {
            self.reject_subset(
                RejectionSite::OpaqueHandleConstructed,
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
            self.reject_subset(
                RejectionSite::AmbientClassConstructed,
                format!(
                    "ambient class `{name}` is obtained from the host, not constructed, because \
                     a `declare class` has no constructor body"
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if self.classes[class_id.0].is_descriptor {
            self.reject_subset(
                RejectionSite::DescriptorClassConstructed,
                format!(
                    "descriptor class `{name}` is constructed with an object literal, not `new`"
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        self.decide_constructor_parameters(class_id);
        let params = self.class_sigs[class_id.0].ctor.clone().unwrap_or_default();
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        let args = self.check_args(
            RejectionSite::ReferenceConstructorArgumentCount,
            &params,
            args_ast,
            fx,
            &pos,
            &name,
        );
        hir::Expr {
            pending_work: None,
            kind: ExprKind::New {
                class: class_id,
                args,
            },
            ty: Type::Class(class_id),
            pos,
        }
    }
}
