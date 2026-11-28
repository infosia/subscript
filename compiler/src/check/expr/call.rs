//! Checks call expressions, method calls, and argument lists.

use crate::check::rejection::RejectionSite;
use oxc_allocator::CloneIn;
use oxc_ast::ast;
use oxc_span::GetSpan;

use crate::check::{source_name, static_member_symbol, Checker, FnCtx, ParamSig, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, AmbientFn, AsyncCallee, Callee, ContextBytesFn, ExprKind, NumFn, WorkerFn};
use crate::types::{ClassId, Type};

impl<'p> Checker<'p> {
    pub(super) fn check_call(
        &mut self,
        c: &ast::CallExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        self.read_call_result(c, fx);
        let active = self.active_call.replace(c.span);
        let active_context = self.enter_call_context(c, ctx);
        let checked = self.checked_call(c, Self::poisoned_call, |checker| {
            checker.check_call_inner(c, ctx, fx, pos)
        });
        self.active_call = active;
        self.active_call_context = active_context;
        checked
    }

    fn check_call_inner(
        &mut self,
        c: &ast::CallExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        if matches!(&c.callee, ast::Expression::Super(_)) {
            let site = if c.type_arguments.is_none()
                && fx
                    .frames
                    .last()
                    .is_some_and(|frame| frame.super_call_available)
            {
                RejectionSite::SuperConstructorCall
            } else {
                RejectionSite::NonExpressionCalleeUnavailable
            };
            self.reject_subset(site, "call form outside the decided surface", pos.clone());
            return self.err_expr(pos);
        }
        let callee = &c.callee;
        let mut callee: &ast::Expression = callee;
        while let ast::Expression::ParenthesizedExpression(p) = callee {
            callee = &p.expression;
        }
        match callee {
            ast::Expression::Identifier(id) => self.check_named_call(id, c, fx, pos, false),
            expression if expression.as_member_expression().is_some() => {
                match expression.as_member_expression() {
                    Some(m) => self.check_method_call(m, c, ctx, fx, pos),
                    None => self.err_expr(pos),
                }
            }
            other => {
                // compiler.md §241 rule 2: a lambda that is the callee runs
                // at once.
                self.lambda_called_at_once =
                    matches!(other, ast::Expression::ArrowFunctionExpression(_));
                let value = self.check_expr(other, None, fx);
                self.lambda_called_at_once = false;
                self.check_indirect_call(value, c, fx, pos)
            }
        }
    }

    pub(super) fn check_named_call(
        &mut self,
        id: &ast::IdentifierReference,
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
        unreachable_statement: bool,
    ) -> hir::Expr {
        self.with_expression_work(|checker| {
            let name = id.name.to_string();
            let ident_pos = checker.pos(id.span);
            if let Some(call) = checker.check_lambda_self_call(id, c, fx, &pos) {
                return call;
            }
            if fx.owns_local_name(&name) {
                let callee = checker.check_ident(id, None, fx);
                return checker.check_indirect_call(callee, c, fx, pos);
            }
            let item = checker.scope_item(&name, &ident_pos, fx);
            if matches!(
                item,
                Some(ScopeItem::Func(_) | ScopeItem::GenericFunc(_) | ScopeItem::Foreign(_))
            ) {
                checker.mark_user_call(c, false);
            }
            if item.is_none() && crate::check::exception::ErrorKind::from_name(&name).is_some() {
                return checker.reject_error_call(&name, pos);
            }
            if c.type_arguments.is_some()
                && matches!(
                    item,
                    Some(ScopeItem::Func(_) | ScopeItem::Foreign(_) | ScopeItem::StandardText(_))
                )
            {
                checker.reject_subset(
                    RejectionSite::NonGenericFunctionTypeArguments,
                    format!("`{}` is not generic", name),
                    ident_pos.clone(),
                );
            }
            match item {
                Some(
                    ScopeItem::Poisoned | ScopeItem::Namespace { .. } | ScopeItem::Interface(_),
                ) => {
                    checker.check_poisoned_arguments(&c.arguments, fx);
                    checker.err_expr(pos)
                }
                Some(ScopeItem::Func(f)) => checker.check_direct_call(&f, c, fx, pos),
                Some(ScopeItem::Foreign(f)) => checker.check_foreign_call(&f, c, fx, pos),
                Some(ScopeItem::StandardFile(write)) => checker.check_file_call(write, c, fx, pos),
                Some(ScopeItem::StandardText(operation)) => {
                    checker.check_text_call(operation, c, fx, pos)
                }
                Some(ScopeItem::GenericFunc(key)) => {
                    let (arguments, checked) = if let Some(type_args) = &c.type_arguments {
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
                            let mut value = checker
                                .check_direct_call_with_arguments(&mono, c, fx, pos, checked);
                            value.ty =
                                checker.call_instance_result(&key, &mono, &arguments.types, true);
                            value
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
                Some(ScopeItem::TypeAlias(_) | ScopeItem::SourceAlias(_)) => {
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
                    // compiler.md §204 rule 1: the global predicate is the `Number` form.
                    if let Some(f @ (NumFn::IsNaN | NumFn::IsFinite)) =
                        crate::ambient::number_static(&name)
                    {
                        return checker.check_number_predicate_call(f, c, fx, pos);
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
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        self.check_direct_call_with_arguments(fn_name, c, fx, pos, None)
    }

    pub(super) fn check_direct_call_with_arguments(
        &mut self,
        fn_name: &str,
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
        checked: Option<Vec<Option<hir::Expr>>>,
    ) -> hir::Expr {
        if !self.defers_call_arguments(self.fn_sigs.get(fn_name).is_some_and(|sig| sig.generic)) {
            self.decide_function_parameters(fn_name);
        }
        let Some(mut sig) = self.fn_sigs.get(fn_name).cloned() else {
            return self.err_expr(pos);
        };
        if let Some((class, _)) = self.static_method_owners.get(fn_name) {
            sig = self.member_signature(*class, sig, fx);
        }
        if sig.is_async {
            let args = self.check_args_with_arguments(
                RejectionSite::SourceFunctionArgumentCount,
                &sig.params,
                &c.arguments,
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
            &c.arguments,
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
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        let Some(sig) = self.foreign_sigs.get(name).cloned() else {
            return self.err_expr(pos);
        };
        let args = self.check_args(
            RejectionSite::ContextMethodArgumentCount,
            &sig.params,
            &c.arguments,
            fx,
            &pos,
            name,
        );
        let completion = self
            .foreign_defs
            .iter()
            .any(|f| f.name == name && f.completion_result.is_some());
        let value = hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Foreign(name.to_string()),
                args,
            },
            ty: sig.ret,
            pos,
        };
        if completion {
            self.track_async_call_result(value, fx)
        } else {
            value
        }
    }

    fn check_ambient_call(
        &mut self,
        ambient: AmbientFn,
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        let mut params: Vec<ParamSig> = crate::ambient::ambient_params(ambient)
            .iter()
            .map(|t| ParamSig::positional(t.clone()))
            .collect();
        let label = if matches!(ambient, AmbientFn::Collect | AmbientFn::UnsafeDelete) {
            format!("Context.{}", ambient.name())
        } else {
            ambient.name().to_string()
        };
        let checked = if ambient == AmbientFn::UnsafeDelete {
            self.check_free_argument(c, &mut params, fx)
        } else {
            None
        };
        let args = self.check_args_with_arguments(
            RejectionSite::AmbientFunctionArgumentCount,
            &params,
            &c.arguments,
            fx,
            &pos,
            &label,
            checked,
            false,
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

    /// Checks the first argument of `Context.free` (compiler.md §358 rule
    /// 1). A dynamic array of an uncounted element type takes its own type
    /// as the parameter; each other argument keeps the `object` parameter.
    /// A counted element type is rejected (§358 rule 5). A type parameter
    /// element takes the verdict of its instance (§143 rule 2a).
    fn check_free_argument(
        &mut self,
        c: &ast::CallExpression,
        params: &mut [ParamSig<'p>],
        fx: &mut FnCtx<'p>,
    ) -> Option<Vec<Option<hir::Expr>>> {
        let argument = c.arguments.first()?;
        if matches!(argument, ast::Argument::SpreadElement(_)) {
            return None;
        }
        let parameter = params.first_mut()?;
        let checked = self.check_expr(
            crate::check::argument_expression(argument),
            Some(&Type::Object),
            fx,
        );
        if let Type::Array(element) = self.apparent_type(&checked.ty) {
            if !self.instance_restriction(
                crate::check::opaque::InstanceRestriction::ArrayElementKind,
                &element,
            ) && self.apparent_type(&element).counted_type().is_some()
            {
                let message = format!(
                    "`Context.free` cannot release an array of the counted type `{}`; \
                     the last holder of the array releases it (§171)",
                    self.type_name(&element)
                );
                self.reject_subset(
                    RejectionSite::ContextFreeCountedArray,
                    message,
                    checked.pos.clone(),
                );
            }
            *parameter = ParamSig::positional(checked.ty.clone());
        }
        Some(vec![Some(checked)])
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
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
        member_pos: Pos,
    ) -> hir::Expr {
        let name = function.name();
        let Some(type_args) = &call.type_arguments else {
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
            // §360 rule 2: a scalar, a FixedArray, and a value class are the
            // accepted kinds; the storage walk below decides each byte.
            let accepted_kind = self.apparent_type(&target).is_numeric()
                || match &self.apparent_type(&target) {
                    Type::FixedArray(..) | Type::Bool | Type::Enum(_) => true,
                    Type::Class(id) => self.classes.get(id.0).is_some_and(|class| class.is_value),
                    _ => false,
                };
            if !accepted_kind {
                let target_name = self.type_name(&target);
                self.reject_subset(RejectionSite::ContextByteTargetKind, format!(
                    "`Context.{name}<T>` cannot use `{target_name}`; it is not a @ValueType value class, a FixedArray, a sized numeric, a boolean, or an enum"
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
        if let Some(spread) = call.arguments.iter().find_map(|argument| match argument {
            ast::Argument::SpreadElement(spread) => Some(spread.span),
            _ => None,
        }) {
            let spread_pos = self.pos(spread);
            self.reject_subset(
                RejectionSite::ContextBytesSpread,
                "spread arguments require variadic parameters, which the language does not have",
                spread_pos.clone(),
            );
            return self.err_expr(spread_pos);
        }
        if call.arguments.len() != params.len() {
            self.reject_subset(
                RejectionSite::ContextBytesArgumentCount,
                format!(
                    "`Context.{name}` expects exactly {} argument(s), got {}",
                    params.len(),
                    call.arguments.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let mut args = Vec::with_capacity(params.len());
        for (argument, expected) in call.arguments.iter().zip(&params) {
            let mut checked = self.check_expr(
                crate::check::argument_expression(argument),
                Some(expected),
                fx,
            );
            if self.involves_type_parameter(expected) || self.involves_type_parameter(&checked.ty) {
                self.require_expr_assignable(
                    &mut checked,
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
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        self.mark_user_call(c, false);
        self.check_indirect_call_with_arguments(callee, c, fx, pos, None)
    }

    fn check_indirect_call_with_arguments(
        &mut self,
        callee: hir::Expr,
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
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
                    let named_call = matches!(
                        super::unparen_expr(&c.callee),
                        ast::Expression::Identifier(_)
                    );
                    let defaults = (c.arguments.len() < ft.params.len() && named_call)
                        .then(|| checker.named_lambda_defaults(&callee, fx))
                        .flatten();
                    let params: Vec<ParamSig> = defaults
                        .as_ref()
                        .map(Self::lambda_default_signature)
                        .unwrap_or_else(|| {
                            ft.params
                                .iter()
                                .map(|t| ParamSig::positional(t.clone()))
                                .collect()
                        });
                    let name = match &callee.kind {
                        ExprKind::Local(name, ..) => name.clone(),
                        ExprKind::Global(symbol, _) => symbol.source_name(),
                        _ => "the function value".to_string(),
                    };
                    let count_site = if defaults.is_some() {
                        RejectionSite::SourceFunctionArgumentCount
                    } else if c.arguments.len() < params.len()
                        && checker
                            .function_value_required(&callee, fx)
                            .is_some_and(|required| c.arguments.len() >= required)
                    {
                        RejectionSite::FunctionValueOptionalArgumentCount
                    } else {
                        RejectionSite::FunctionValueArgumentCount
                    };
                    let args = checker.check_args_with_arguments(
                        count_site,
                        &params,
                        &c.arguments,
                        fx,
                        &pos,
                        &name,
                        checked_arguments,
                        false,
                    );
                    let value = hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Call {
                            callee: if let Some(defaults) = defaults {
                                Callee::NamedLambda {
                                    value: Box::new(callee),
                                    defaults,
                                }
                            } else {
                                Callee::Value(Box::new(callee))
                            },
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
        receiver: &ast::Expression,
        name: &str,
        fx: &FnCtx<'p>,
    ) -> bool {
        let ast::Expression::Identifier(receiver) = receiver else {
            return false;
        };
        if fx.owns_local_name(receiver.name.as_ref()) {
            return false;
        }
        match self.peek_scope_item(receiver.name.as_ref()) {
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
        member: &ast::MemberExpression,
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
        member_pos: Pos,
        name: &str,
    ) -> Option<hir::Expr> {
        let view = super::member_view(member);
        let member = &view;
        if self.rejected_static_generic_method(member.object, name, fx) {
            self.mark_user_call(call, true);
            return Some(self.err_expr(pos));
        }
        let ast::Expression::Identifier(receiver) = member.object else {
            return None;
        };
        let class_name = receiver.name.to_string();
        if fx.owns_local_name(&class_name) {
            return None;
        }
        let receiver_pos = self.pos(receiver.span);
        let Some(ScopeItem::Class(class)) = self.scope_item(&class_name, &receiver_pos, fx) else {
            return None;
        };
        self.mark_user_call(call, true);
        if self.reject_member_access(class, name, true, false, fx, member_pos.clone()) {
            return Some(self.err_expr(pos));
        }
        if self.class_sigs[class.0].has_static_accessor(name) {
            return None;
        }
        // §82.4 rule 2: a static generic method instantiates at the call.
        if self.class_sigs[class.0].has_generic_method(name, true) {
            let Some(instance) =
                self.instantiate_generic_method_call(class, name, call, true, fx, member_pos)
            else {
                return Some(self.err_expr(pos));
            };
            let symbol = static_member_symbol(class, &self.classes[class.0].name, &instance.name);
            let mut value =
                self.check_direct_call_with_arguments(&symbol, call, fx, pos, instance.arguments);
            value.ty = instance.result;
            return Some(value);
        }
        if !self.class_sigs[class.0].static_methods.contains_key(name) {
            return None;
        }
        if call.type_arguments.is_some() {
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
        m: &ast::MemberExpression,
        c: &ast::CallExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        let view = super::member_view(m);
        let m = &view;
        if self.reject_caught_property(m.object, fx) || self.reject_static_this_member(m, fx) {
            return self.err_expr(pos);
        }
        let super::MemberKey::Ident(prop) = &m.prop else {
            let value = self.check_member_read(m, fx);
            return self.check_indirect_call(value, c, fx, pos);
        };
        if fx.field_initializer.is_some()
            && matches!(
                super::unparen_expr(m.object),
                ast::Expression::ThisExpression(_)
            )
        {
            let value = self.check_member_read(m, fx);
            return self.check_indirect_call(value, c, fx, pos);
        }
        let name = prop.name.to_string();
        let prop_pos = self.pos(prop.span);
        if self.ambient_namespace(m.object, fx) == Some("Promise") {
            match name.as_str() {
                "all" => return self.check_promise_all(c, hir::AsyncAggregate::All, fx, pos),
                "race" => return self.check_promise_all(c, hir::AsyncAggregate::Race, fx, pos),
                _ => {}
            }
            self.reject_subset(
                RejectionSite::PromiseStaticCall,
                format!("Promise static `Promise.{name}(...)` is not in the language"),
                prop_pos.clone(),
            );
            return self.err_expr(pos);
        }
        if matches!(m.object, ast::Expression::Identifier(id) if id.name.as_ref() == "Worker")
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
        if self.is_context_namespace(m.object, fx) {
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
        if self.is_math_namespace(m.object, fx) {
            if let Some(f) = crate::ambient::math_fn(&name) {
                return self.check_math_call(f, c, fx, pos);
            }
        }
        // Accepted Number statics (stdlib.md §11.1/§11.3, Q27) are
        // resolved before generic namespace-member handling. Parser
        // spellings share the globals' NumFn/runtime identity.
        if self.is_number_namespace(m.object, fx) {
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
        if self.is_json_namespace(m.object, fx) {
            return self.check_json_call(&name, c, ctx, fx, pos, prop_pos);
        }
        // `Date.UTC(…)` / `Date.now()` (stdlib.md §3): static intrinsic
        // calls, resolved before the generic namespace-member path.
        if self.is_date_namespace(m.object, fx) {
            if let Some(handled) = self.check_date_static_call(&name, c, fx, pos.clone()) {
                return handled;
            }
        }
        if matches!(m.object, ast::Expression::Identifier(id) if id.name.as_ref() == "Map")
            && self.assoc_is_ambient("Map", fx)
            && name == "groupBy"
        {
            return self.check_map_group_by(c, fx, pos);
        }
        // Resolves Array namespace calls (stdlib.md §9.11; compiler.md §105).
        if self.ambient_namespace(m.object, fx) == Some("Array") {
            return self.check_array_static_call(&name, c, ctx, fx, pos, prop_pos);
        }
        if let Some(handled) =
            self.check_namespace_member(m.object, &name, prop_pos.clone(), fx, false)
        {
            // Enum members are values, not callables.
            if matches!(self.apparent_type(&handled.ty), Type::Error) {
                return handled;
            }
            return self.check_indirect_call(handled, c, fx, pos);
        }
        let recv = self.check_receiver(m.object, fx);
        // §186 rule 5: the receiver type selects a reaction, not the name.
        if matches!(name.as_str(), "then" | "catch" | "finally")
            && matches!(self.apparent_type(&recv.ty), Type::AsyncHandle(_))
        {
            return self.check_promise_reaction(recv, prop, c, fx, pos);
        }
        self.check_method_call_on(recv, prop, c, fx, pos)
    }

    /// Instantiates a generic method and keeps actual result substitutions (compiler.md §335 rule 1).
    pub(super) fn instantiate_generic_method_call(
        &mut self,
        class: ClassId,
        name: &str,
        call: &ast::CallExpression,
        is_static: bool,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> Option<crate::check::constraint_call::MethodInstance> {
        self.mark_user_call(call, true);
        if self.class_sigs[class.0].generic_method_is_rejected(name, is_static) {
            return None;
        }
        let (arguments, checked) = match &call.type_arguments {
            Some(type_args) => (self.resolve_instance_arguments(type_args), None),
            None => {
                let (arguments, checked) =
                    self.infer_method_arguments(class, name, call, is_static, fx, &pos)?;
                (arguments, Some(checked))
            }
        };
        let result_arguments =
            self.method_result_arguments(class, name, is_static, &arguments.types);
        let instance = self.instantiate_method(class, name, &arguments, is_static, pos)?;
        let signatures = if is_static {
            &self.class_sigs[class.0].static_methods
        } else {
            &self.class_sigs[class.0].methods
        };
        let signature = signatures.get(&instance)?;
        let result = signature
            .result_template
            .as_ref()
            .unwrap_or(&signature.ret)
            .substitute_instances(&result_arguments);
        Some(crate::check::constraint_call::MethodInstance {
            name: instance,
            arguments: checked,
            result,
        })
    }

    /// Checks `receiver.name(...)` from an already-checked receiver.
    pub(in crate::check) fn check_method_call_on(
        &mut self,
        recv: hir::Expr,
        property: &ast::IdentifierName,
        c: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
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
        if let Some(value) = checker.check_constrained_method_call(&recv, property, c, fx, &pos, false) {
            return value;
        }
        let mut recv = checker.apparent_expr(recv);
        checker.convert_string_operand(&mut recv, crate::check::string_operand::StringOperand::Member);
        // §231 rule 1: a user class or an interface constraint has user methods.
        if matches!(checker.apparent_type(&recv.ty), Type::Class(id) if id != checker.error_class)
            || matches!(checker.apparent_type(&recv.ty), Type::Interface(_))
        {
            checker.mark_user_call(c, true);
        }
        let mut name = property.name.to_string();
        let mut inferred = None;
        let mut result_type = None;
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
            checker.check_poisoned_arguments(&c.arguments, fx);
            return checker.err_expr(pos);
        }
        // §82.4 rule 3: the call names the instance, not the template.
        if let Type::Class(class) = &checker.apparent_type(&recv.ty) {
            let class = *class;
            if checker.reject_member_access(class, &name, false, false, fx, prop_pos.clone()) {
                return checker.err_expr(pos);
            }
            if checker.class_sigs[class.0].has_generic_method(&name, false) {
                let Some(instance) = checker.instantiate_generic_method_call(
                    class,
                    &name,
                    c,
                    false,
                    fx,
                    prop_pos.clone(),
                ) else {
                    return checker.err_expr(pos);
                };
                name = instance.name;
                inferred = instance.arguments;
                result_type = Some(instance.result);
            }
        }
        if checker.is_error_type(&recv.ty) && name == "toString" {
            checker.check_args(
                RejectionSite::ScalarToStringArgumentCount,
                &[],
                &c.arguments,
                fx,
                &pos,
                &name,
            );
            if c.type_arguments.is_some() {
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
            Type::TaskGroup => checker.check_task_group_method(recv, &name, c, fx, pos),
            Type::Interface(interface) => {
                checker.check_interface_method_call(recv, &interface, property, c, fx, pos)
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
                            annotated: false,
                            name: "message".to_string(),
                            state: crate::check::initializer::TypeState::decided((*input).clone()),
                            initializer: None,
                            has_default: false,
                        }],
                        Type::Void,
                    ),
                    "poll" => (
                        WorkerFn::Poll,
                        Vec::new(),
                        Type::Nullable(
                            output.clone(),
                            crate::ambient::nullable_result("Worker", "poll"),
                        ),
                    ),
                    "close" => (WorkerFn::Close, Vec::new(), Type::Void),
                    "join" => (WorkerFn::Join, Vec::new(), Type::Void),
                    // compiler.md §349 rule 1: an ordinary handle.
                    "receive" => (
                        WorkerFn::Receive,
                        Vec::new(),
                        Type::async_handle(Type::Nullable(
                            output.clone(),
                            crate::ambient::nullable_result("Worker", "receive"),
                        )),
                    ),
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
                    &c.arguments,
                    fx,
                    &pos,
                    &name,
                ));
                let value = hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Call {
                        callee: Callee::Worker(function),
                        args,
                    },
                    ty: ret,
                    pos,
                };
                checker.track_async_call_result(value, fx)
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
                    &c.arguments,
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
                    ty: Type::nullable_spelled(
                        *message,
                        crate::ambient::nullable_result("Inbox", &name),
                    ),
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
                    annotated: false,
                    name: "message".to_string(),
                    state: crate::check::initializer::TypeState::decided((*message).clone()),
                    initializer: None,
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(checker.check_args(
                    RejectionSite::FixedArrayPushArgumentCount,
                    &params,
                    &c.arguments,
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
                "push" => checker.check_array_push(recv, (*elem).clone(), c, fx, pos),
                "pop" => {
                    let args = checker.check_args(
                        RejectionSite::ArrayPopArgumentCount,
                        &[],
                        &c.arguments,
                        fx,
                        &pos,
                        "pop",
                    );
                    let ty = checker.removal_result((*elem).clone(), "pop");
                    mk(recv, args, ty, pos)
                }
                other => {
                    // The §9 method intrinsics (stdlib.md §9, Q22).
                    if other == "toString" && !c.arguments.is_empty() {
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
                    if f.on_fixed_array() {
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
                "normalize" => checker.check_normalize(recv, c, pos, prop_pos),
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
                        &c.arguments,
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
                    Some(mut sig) => {
                        sig.ret = result_type.clone().unwrap_or(sig.ret);
                        sig = checker.member_signature(id, sig, fx);
                        if sig.is_async {
                            let args = checker.check_args_with_arguments(
                                RejectionSite::AsyncMethodArgumentCount,
                                &sig.params,
                                &c.arguments,
                                fx,
                                &pos,
                                &name,
                                                        inferred.take(),
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
                            &c.arguments,
                            fx,
                            &pos,
                            &name,
                                                inferred.take(),
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

    pub(in crate::check) fn check_poisoned_arguments(
        &mut self,
        args: &[ast::Argument],
        fx: &mut FnCtx<'p>,
    ) {
        for argument in args {
            let _ = self.check_expr(crate::check::argument_expression(argument), None, fx);
        }
    }

    /// Reports an argument count outside `(total, required)` for the
    /// callee `what`. A spread argument gives no count.
    pub(super) fn check_argument_count(
        &mut self,
        site: RejectionSite,
        (total, required): (usize, usize),
        args: &[ast::Argument],
        pos: &Pos,
        what: &str,
    ) {
        let has_spread = args
            .iter()
            .any(|arg| matches!(arg, ast::Argument::SpreadElement(_)));
        if !has_spread && (args.len() < required || args.len() > total) {
            if matches!(
                site,
                RejectionSite::FunctionValueArgumentCount
                    | RejectionSite::FunctionValueOptionalArgumentCount
            ) {
                self.reject_subset(site, format!("`{what}` expects {total} argument(s), got {}: a call through a function value passes every parameter", args.len()), pos.clone());
                return;
            }
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
        params: &[ParamSig<'p>],
        args: &[ast::Argument],
        fx: &mut FnCtx<'p>,
        pos: &Pos,
        what: &str,
    ) -> Vec<hir::Expr> {
        self.check_args_with_arguments(site, params, args, fx, pos, what, None, false)
    }

    pub(in crate::check) fn check_args_with_arguments(
        &mut self,
        site: RejectionSite,
        params: &[ParamSig<'p>],
        args: &[ast::Argument],
        fx: &mut FnCtx<'p>,
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
                    value.clone().unwrap_or_else(|| {
                        self.err_expr(self.pos(crate::check::argument_expression(argument).span()))
                    })
                })
                .collect();
            self.defer_work(
                crate::check::initializer::DeferredWork::Arguments {
                    source: std::rc::Rc::new(
                        args.iter()
                            .map(|argument| argument.clone_in(self.prog.allocator))
                            .collect(),
                    ),
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
        let required = params
            .iter()
            .rposition(|p| !p.has_default)
            .map_or(0, |i| i + 1);
        if params
            .iter()
            .all(|parameter| self.apparent_type(parameter.ty()) != Type::Error)
        {
            self.check_argument_count(site, (params.len(), required), args, pos, what);
        }
        let mut out = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let prechecked = checked.next().flatten();
            if let ast::Argument::SpreadElement(spread) = arg {
                let p = self.pos(spread.span);
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
                self.cycle(self.pos(crate::check::argument_expression(arg).span()));
            }
            let param_ty = params.get(i).map(|p| p.ty().clone());
            // compiler.md §252 rule 1: a call with its type arguments, given
            // or inferred, reads the type of the place after the
            // substitution.
            let callback_context = generic_callee
                && crate::check::inference::generic_callback_place(
                    param_ty
                        .as_ref()
                        .and_then(|ty| self.lambda_context(ty))
                        .map(|function| function.params.len()),
                    crate::check::argument_expression(arg),
                );
            let saved_context =
                std::mem::replace(&mut self.generic_callback_context, callback_context);
            let mut checked = prechecked.unwrap_or_else(|| {
                self.check_expr(
                    crate::check::argument_expression(arg),
                    param_ty.as_ref(),
                    fx,
                )
            });
            self.generic_callback_context = saved_context;
            if let Some(param_ty) = param_ty {
                self.require_expr_assignable(&mut checked, &param_ty, fx, "the argument");
                if self.transfers_async_origins(&param_ty) {
                    let origins = self.expr_async_origins(&checked, fx);
                    fx.handle_async_origins(&origins);
                }
            }
            out.push(checked);
        }
        out
    }
}
