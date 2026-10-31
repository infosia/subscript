//! Checks literals, template strings, and identifier references.

use crate::check::rejection::RejectionSite;
use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, Callee, ExprKind, RegexFn, TplPart};
use crate::types::Type;

use super::{path_key, synthesized_int_range};

/// C4: a numeric literal spelled with a `.` or a decimal exponent is
/// fractional.
fn is_fractional(n: &ast::Number) -> bool {
    let raw: &str = n.raw.as_ref().map(|a| a.as_ref()).unwrap_or("");
    let hex = raw.starts_with("0x") || raw.starts_with("0X");
    raw.contains('.') || (!hex && (raw.contains('e') || raw.contains('E')))
}

/// The type that C4 gives a literal with no context (`compiler.md`
/// §198.1 rule 6): `i32` or `f64` for a numeric literal, also under a
/// unary minus as the checker folds it, `string`, or `boolean`. Any
/// other expression gives `None`.
pub(in crate::check) fn literal_default_type(expr: &ast::Expr) -> Option<Type> {
    let mut expr = expr;
    while let ast::Expr::Paren(paren) = expr {
        expr = &paren.expr;
    }
    if let ast::Expr::Unary(ast::UnaryExpr {
        op: ast::UnaryOp::Minus,
        arg,
        ..
    }) = expr
    {
        let mut arg: &ast::Expr = arg;
        while let ast::Expr::Paren(paren) = arg {
            arg = &paren.expr;
        }
        return match arg {
            ast::Expr::Lit(ast::Lit::Num(n)) => Some(numeric_literal_type(n)),
            _ => None,
        };
    }
    match expr {
        ast::Expr::Lit(ast::Lit::Num(n)) => Some(numeric_literal_type(n)),
        ast::Expr::Lit(ast::Lit::Str(_)) => Some(Type::Str),
        ast::Expr::Lit(ast::Lit::Bool(_)) => Some(Type::Bool),
        _ => None,
    }
}

fn numeric_literal_type(n: &ast::Number) -> Type {
    if is_fractional(n) {
        Type::F64
    } else {
        Type::I32
    }
}

impl<'p> Checker<'p> {
    pub(super) fn check_lit(&mut self, lit: &ast::Lit, ctx: Option<&Type>, pos: Pos) -> hir::Expr {
        match lit {
            ast::Lit::Num(n) => self.check_num_lit(n, false, ctx, pos),
            ast::Lit::Str(s) => {
                let value = s.value.to_string();
                if let Some(Type::StringAlias(id)) = ctx.map(|ty| self.apparent_type(ty)).as_ref() {
                    if let Some(discriminant) = self.string_aliases.get(id.0).and_then(|alias| {
                        alias
                            .members
                            .iter()
                            .position(|member| member == &value)
                            .and_then(|index| alias.member_discriminant(index))
                    }) {
                        return hir::Expr {
                            pending_work: None,
                            kind: ExprKind::Int(discriminant),
                            ty: Type::StringAlias(*id),
                            pos,
                        };
                    }
                }
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Str(value),
                    ty: Type::Str,
                    pos,
                }
            }
            ast::Lit::Bool(b) => hir::Expr {
                pending_work: None,
                kind: ExprKind::Bool(b.value),
                ty: Type::Bool,
                pos,
            },
            ast::Lit::Null(_) => hir::Expr {
                pending_work: None,
                kind: ExprKind::Null,
                ty: Type::Null,
                pos,
            },
            ast::Lit::Regex(regex) => {
                let pattern = regex.exp.to_string();
                let flags = regex.flags.to_string();
                if flags.contains('v') {
                    self.reject_subset(RejectionSite::RegexLiteralUnicodeSetsFlag, "the `v` flag requires ES2024 in a regex literal; use `new RegExp(pattern, \"v\")`", pos.clone());
                    return self.err_expr(pos);
                }
                if flags.contains('y') {
                    self.reject_subset(
                        RejectionSite::RegexSticky,
                        "`RegExp.lastIndex` is not in the language: sticky matching requires reading and writing that mutable state (Q31)",
                        pos.clone()
);
                    return self.err_expr(pos);
                }
                if let Err(error) = crate::regex::validate_literal(&pattern, &flags) {
                    self.reject_subset(
                        error.site,
                        format!("invalid regular-expression literal: {error}"),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let key = (pos.file.clone(), pos.line, pos.col);
                let name = if let Some(name) = self.regex_literals.get(&key) {
                    name.clone()
                } else {
                    let name = loop {
                        let name =
                            format!("__subscript_regex_literal_{}", self.next_regex_literal_id);
                        self.next_regex_literal_id += 1;
                        if !self.global_sigs.contains_key(&name) {
                            break name;
                        }
                    };
                    let init = hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Call {
                            callee: Callee::Regex(RegexFn::New),
                            args: vec![
                                hir::Expr {
                                    pending_work: None,
                                    kind: ExprKind::Str(pattern),
                                    ty: Type::Str,
                                    pos: pos.clone(),
                                },
                                hir::Expr {
                                    pending_work: None,
                                    kind: ExprKind::Str(flags),
                                    ty: Type::Str,
                                    pos: pos.clone(),
                                },
                            ],
                        },
                        ty: Type::RegExp,
                        pos: pos.clone(),
                    };
                    self.globals.push(hir::Global {
                        symbol: hir::Symbol::from_full_text(name.clone()),
                        name: name.clone(),
                        ty: Type::RegExp,
                        mutable: false,
                        init,
                        // compiler.md §137 rule 3b: regex globals precede all module work.
                        initializer_index: 0,
                        pos: pos.clone(),
                    });
                    self.regex_literals.insert(key, name.clone());
                    name
                };
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Global(hir::Symbol::from_full_text(name)),
                    ty: Type::RegExp,
                    pos,
                }
            }
            other => {
                let p = self.pos(other.span());
                self.reject_subset(
                    RejectionSite::BigIntLiteral,
                    "literal form outside the decided surface",
                    p.clone(),
                );
                self.err_expr(p)
            }
        }
    }

    /// Contextual numeric literal typing (C4): integer literals adopt
    /// the sized type of their context and are range-checked; fractional
    /// literals adopt the contextual float type and are an error in an
    /// integer context. Context-free defaults: `i32` / `f64`.
    pub(super) fn check_num_lit(
        &mut self,
        n: &ast::Number,
        negate: bool,
        ctx: Option<&Type>,
        pos: Pos,
    ) -> hir::Expr {
        let raw: &str = n.raw.as_ref().map(|a| a.as_ref()).unwrap_or("");
        let fractional = is_fractional(n);
        let value = if negate { -n.value } else { n.value };
        let target = match ctx {
            Some(t)
                if self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::SizedNumeric,
                    t,
                ) =>
            {
                Type::GenericNumber
            }
            Some(t) if self.apparent_type(t).is_numeric() => self.apparent_type(t),
            _ => {
                if fractional {
                    Type::F64
                } else {
                    Type::I32
                }
            }
        };
        // §143: a numeric parameter context gives a literal no concrete storage width.
        if matches!(self.apparent_type(&target), Type::GenericNumber) {
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Float(value),
                ty: target,
                pos,
            };
        }
        if self.apparent_type(&target).is_float() {
            // Round-to-nearest-even first overflows binary16 at the
            // midpoint 65520: values below it still round to 65504.
            if self.apparent_type(&(target)) == Type::F16 && value.abs() >= 65_520.0 {
                self.reject_subset(
                    RejectionSite::HalfFloatLiteralRange,
                    format!("numeric literal {} out of range for `f16`", raw),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Float(value),
                ty: target,
                pos,
            };
        }
        if fractional {
            let name = self.type_name(&target);
            self.reject_subset(
                RejectionSite::FractionalIntegerLiteral,
                format!("fractional literal in integer context `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let integer = if let Some(raw) = n.raw.as_deref() {
            let (lo, hi) = self
                .apparent_type(&target)
                .int_bounds()
                .unwrap_or((i128::from(i64::MIN), i128::from(i64::MAX)));
            crate::check::parse_integer_spelling(raw, negate)
                .filter(|value| *value >= lo && *value <= hi)
                .map(|value| value as i64)
        } else {
            // A synthesized node has no spelling, so the range check reads
            // its `f64` value (§56.1).
            let (lo, hi) = synthesized_int_range(&target).unwrap_or((i64::MIN, i64::MAX));
            (value >= lo as f64 && value <= hi as f64).then_some(value as i64)
        };
        let Some(integer) = integer else {
            let name = self.type_name(&target);
            self.reject_subset(
                RejectionSite::IntegerLiteralRange,
                format!("integer literal {} out of range for `{}`", raw, name),
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Int(integer),
            ty: target,
            pos,
        }
    }

    pub(super) fn check_template(&mut self, tpl: &ast::Tpl, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let mut parts = Vec::new();
        for (i, quasi) in tpl.quasis.iter().enumerate() {
            let text = quasi
                .cooked
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or_else(|| quasi.raw.to_string());
            if !text.is_empty() {
                parts.push(TplPart::Text(text));
            }
            if let Some(e) = tpl.exprs.get(i) {
                let mut checked = self.check_expr(e, None, fx);
                if matches!(self.apparent_type(&checked.ty), Type::Array(_)) {
                    checked = self.template_array_join(checked, e.span(), fx);
                }
                let printable = self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::TemplateInterpolation,
                    &checked.ty,
                ) || self.apparent_type(&checked.ty).is_numeric()
                    || matches!(
                        self.apparent_type(&checked.ty),
                        Type::Str | Type::Bool | Type::Enum(_) | Type::StringAlias(_) | Type::Error
                    );
                if !printable && self.apparent_type(&checked.ty) == Type::Date {
                    // Q20: a Date has no implicit string form (the lib's
                    // would be local-time `toString`).
                    self.reject_api_form(
                        "Date",
                        "template interpolation",
                        "Date template interpolation",
                        checked.pos.clone(),
                    );
                } else if !printable {
                    let name = self.type_name(&checked.ty);
                    self.reject_subset(
                        RejectionSite::TemplateInterpolationKind,
                        format!(
                            "type `{}` cannot be interpolated into a template{}",
                            name,
                            self.null_test_fix(&[&checked.ty])
                        ),
                        checked.pos.clone(),
                    );
                }
                parts.push(TplPart::Expr(checked));
            }
        }
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Template(parts),
            ty: Type::Str,
            pos,
        }
    }

    /// compiler.md §204 rule 2: an `Array` operand of a template is
    /// `operand.join(",")`. The checked operand is the receiver, so it
    /// evaluates once. A `FixedArray` operand is not in this rule.
    fn template_array_join(
        &mut self,
        operand: hir::Expr,
        span: swc_common::Span,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        // `join` with no argument takes the separator ",".
        let call = ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(ast::Expr::Invalid(ast::Invalid { span }))),
            args: Vec::new(),
            type_args: None,
        };
        let property = ast::IdentName {
            span,
            sym: "join".into(),
        };
        let pos = operand.pos.clone();
        self.check_method_call_on(operand, &property, &call, fx, pos)
    }

    /// compiler.md §216 rule 1: the receiver of `join` on a string-literal
    /// alias element is `receiver.map((x) => `${x}`)`. The contextual
    /// parameter type carries the alias, so the alias needs no name in
    /// the scope of the call. The synthetic nodes take the span of the
    /// `join` call. No node of the callback is a call, so `active_call`
    /// and `operand_calls` do not read that span.
    pub(super) fn string_alias_elements(
        &mut self,
        receiver: hir::Expr,
        element: Type,
        span: swc_common::Span,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> hir::Expr {
        let ident = ast::Ident::new("__element".into(), span, Default::default());
        let text = ast::Expr::Tpl(ast::Tpl {
            span,
            exprs: vec![Box::new(ast::Expr::Ident(ident.clone()))],
            quasis: [false, true]
                .into_iter()
                .map(|tail| ast::TplElement {
                    span,
                    tail,
                    cooked: Some("".into()),
                    raw: "".into(),
                })
                .collect(),
        });
        let callback = ast::ArrowExpr {
            span,
            ctxt: Default::default(),
            params: vec![ast::Pat::Ident(ident.into())],
            body: Box::new(ast::BlockStmtOrExpr::Expr(Box::new(text))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        };
        let call = ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(ast::Expr::Invalid(ast::Invalid { span }))),
            args: vec![ast::ExprOrSpread {
                spread: None,
                expr: Box::new(ast::Expr::Arrow(callback)),
            }],
            type_args: None,
        };
        self.check_array_method(
            receiver,
            element,
            (hir::ArrFn::Map, "map"),
            &call,
            fx,
            pos.clone(),
        )
    }

    /// The value of a module function or a static method (compiler.md
    /// §207.1 rule 1): a function pointer (C5). `direct_value` marks the
    /// whole initializer of a module variable.
    pub(super) fn module_function_value(
        &mut self,
        f: String,
        direct_value: bool,
        pos: Pos,
    ) -> hir::Expr {
        let value_decision = std::mem::replace(&mut self.function_value_decision, direct_value);
        self.decide_function_parameters(&f);
        self.function_value_decision = value_decision;
        let Some(sig) = self.fn_sigs.get(&f).cloned() else {
            return self.err_expr(pos);
        };
        if sig
            .params
            .iter()
            .any(|p| matches!(p.state, crate::check::initializer::TypeState::InProgress))
        {
            if direct_value {
                self.parameter_value_cycle(pos.clone());
            } else {
                self.cycle(pos.clone());
            }
            return self.err_expr(pos);
        }
        if sig.is_generator {
            self.reject_subset(
                RejectionSite::GeneratorFunctionValue,
                "generators may only be called, not passed as values",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if self.undefined_result_value(&sig.ret, &pos) {
            return self.err_expr(pos);
        }
        let result = if sig.is_async {
            Type::AsyncHandle(Box::new(sig.ret))
        } else {
            sig.ret
        };
        let ty = Type::func(sig.params.iter().map(|p| p.ty().clone()).collect(), result);
        hir::Expr {
            pending_work: None,
            kind: ExprKind::FuncRef(hir::Symbol::from_full_text(f)),
            ty,
            pos,
        }
    }

    pub(super) fn check_ident(
        &mut self,
        id: &ast::Ident,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let name = id.sym.to_string();
        let pos = self.pos(id.span);
        if name == "undefined" {
            self.reject_subset(
                if matches!(
                    ctx.map(|ty| self.apparent_type(ty)),
                    Some(Type::StringAlias(_))
                ) {
                    RejectionSite::UndefinedOptionalMember
                } else {
                    RejectionSite::UndefinedValue
                },
                "`undefined` is legal only in `return undefined;` and in an absence comparison",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if let Some(scope) = fx.scopes.iter().rposition(|s| {
            s.vars.contains_key(&name)
                || s.pending.contains(&name)
                || s.switch_declarations.contains_key(&name)
        }) {
            if fx.parameter_decisions.contains(&(scope, name.clone())) {
                self.cycle(pos.clone());
                return self.err_expr(pos);
            }
        }
        if let Some(local) = self.lookup_local(&name, &pos, fx) {
            if self.reject_caught_read(&name, &local, &pos, fx, false) {
                return self.err_expr(pos);
            }
            let mut expr = hir::Expr {
                pending_work: None,
                kind: ExprKind::Local(name, local.ty.clone(), local.annotated),
                ty: local.ty,
                pos,
            };
            self.apply_narrowing(&mut expr, fx);
            return expr;
        }
        if matches!(
            self.type_scope_item(&name),
            Some(ScopeItem::Namespace {
                module: Some(_),
                ..
            })
        ) {
            let message = format!(
                "namespace import `{name}` is a static qualifier and cannot be used as a value"
            );
            if ctx.is_none() {
                self.reject_subset(RejectionSite::UnknownValueName, message, pos.clone());
            } else {
                self.reject_subset(
                    if matches!(
                        ctx.map(|ty| self.apparent_type(ty)),
                        Some(Type::Object | Type::Class(_) | Type::Error)
                            | Some(Type::Nullable(_, _))
                    ) {
                        RejectionSite::NamespaceContextValue
                    } else {
                        RejectionSite::NamespaceIncompatibleContext
                    },
                    message,
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        }
        let item = self.scope_item(&name, &pos);
        match item {
            Some(ScopeItem::Poisoned | ScopeItem::Namespace { .. } | ScopeItem::Interface(_)) => {
                self.err_expr(pos)
            }
            Some(ScopeItem::Global(g)) => {
                // A mirror flag member (§13.2) folds to its C value here, so
                // both tiers emit an immediate rather than reading a global.
                if let Some((value, ty)) = self.ambient_int_consts.get(&g).cloned() {
                    return hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Int(value),
                        ty,
                        pos,
                    };
                }
                let ty = self.decide_global(&g, pos.clone());
                let mut expr = hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Global(hir::Symbol::from_full_text(g)),
                    ty,
                    pos,
                };
                self.apply_narrowing(&mut expr, fx);
                expr
            }
            Some(ScopeItem::Func(f)) => {
                let direct_value = self.initializer_root == Some(id.span);
                self.module_function_value(f, direct_value, pos)
            }
            Some(ScopeItem::GenericFunc(_)) => {
                self.reject_subset(
                    RejectionSite::GenericFunctionValue,
                    format!(
                        "generic function `{}` has no first-class value; call it directly or use a lambda",
                        name
                    ),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::Class(_)) | Some(ScopeItem::GenericClass(_)) => {
                self.reject_subset(
                    RejectionSite::ClassRuntimeValue,
                    format!("class `{}` used as a value", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::Enum(_)) => {
                self.reject_subset(
                    RejectionSite::EnumObjectValue,
                    format!("enum `{}` used as a value; use a member", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::TypeAlias(_) | ScopeItem::SourceAlias(_)) => {
                self.reject_subset(
                    RejectionSite::MirrorTypeAliasValue,
                    format!("type alias `{name}` used as a value"),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::StringAlias(_)) => {
                self.reject_subset(
                    RejectionSite::LiteralAliasValue,
                    format!("string-literal union alias `{name}` used as a value"),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::StandardFile(_)) => {
                self.reject_subset(
                    RejectionSite::FileModuleFunctionValue,
                    format!(
                        "`{name}` from node:fs/promises can only be called; {}",
                        crate::check::file_module::FORMS
                    ),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::StandardText(_)) => self.reject_text_function_value(&name, pos),
            Some(ScopeItem::Foreign(_)) => {
                self.reject_subset(
                    RejectionSite::ForeignFunctionValue,
                    format!("foreign function `{}` may only be called", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            None => {
                if name == "NaN" || name == "Infinity" {
                    // Local and program declarations shadow ambient constants.
                    hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Float(if name == "NaN" {
                            f64::NAN
                        } else {
                            f64::INFINITY
                        }),
                        ty: Type::F64,
                        pos,
                    }
                } else if name == "eval" || name == "Function" {
                    self.reject_subset(
                        RejectionSite::DynamicEvaluatorValue,
                        "no dynamic code evaluation",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Context" {
                    self.reject_subset(
                        RejectionSite::ContextValue,
                        "`Context` is an ambient namespace, not a value; use \
                         `Context.collect()`, `Context.free(value)`, or await \
                         `Context.suspend()` (Q6/Q7/Q34)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Array" {
                    // compiler.md §105.1 rule 1: the builtin namespace
                    // resolves like any other name, and a namespace is
                    // not a value.
                    self.reject_api_form(
                        "Array",
                        "Array used as a value",
                        "Array used as a value",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Math" {
                    // The ambient namespace is not a value (Q19): it
                    // cannot be assigned, passed, or stored.
                    self.reject_api_form(
                        "Math",
                        "Math used as a value",
                        "Math used as a value",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Number" {
                    self.reject_subset(
                        RejectionSite::NumberValue,
                        "`Number` is an ambient namespace, not a value or coercion; \
                         use `Number.<member>` (Q25)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "JSON" {
                    self.reject_subset(
                        RejectionSite::JsonValue,
                        "`JSON` is an ambient namespace, not a value; use \
                         `JSON.stringify(value)` or `JSON.parse<T>(text)` (Q28)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Date" {
                    // The ambient Date surface is a type and a namespace,
                    // never a value (Q20).
                    self.reject_subset(
                        RejectionSite::DateValue,
                        "`Date` is not a value; only `new Date(ms)`, `Date.UTC(…)`, \
                         and `Date.now()` are accepted (Q20)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Map" || name == "Set" {
                    self.reject_subset(
                        RejectionSite::MapSetValue,
                        format!(
                            "`{name}` is a generic reference class, not a value; \
                             construct it with explicit type arguments (Q24)"
                        ),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if crate::ambient::ambient_fn(&name).is_some() {
                    self.reject_subset(
                        RejectionSite::AmbientFunctionValue,
                        format!("ambient function `{}` may only be called", name),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if crate::ambient::number_global(&name).is_some()
                    || name == "isNaN"
                    || name == "isFinite"
                {
                    self.reject_subset(
                        RejectionSite::NumberGlobalValue,
                        format!("`{name}` may only be called, not read as a value (Q25)"),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else {
                    self.reject_subset(
                        if crate::ambient::lib_value_name(&name) {
                            RejectionSite::UnknownAmbientValueName
                        } else {
                            RejectionSite::UnboundValueName
                        },
                        format!("unknown name `{}`", name),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                }
            }
        }
    }

    /// Rewrites a nullable expression to its narrowed type when its
    /// path is in the current non-null set (C7).
    pub(super) fn apply_narrowing(&self, e: &mut hir::Expr, fx: &FnCtx) {
        if matches!(self.apparent_type(&e.ty), Type::Nullable(_, _)) {
            if let Some(key) = path_key(e) {
                if fx
                    .narrowed
                    .get(&key)
                    .is_some_and(|fact| fact.narrows_type())
                {
                    e.ty = self.non_null_type(&e.ty);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::literal_default_type;
    use crate::types::Type;
    use swc_ecma_ast as ast;

    fn default_of(source: &str) -> ast::Expr {
        let program = crate::parse::parse_program(&[crate::SourceFile::entry(
            "main.ts",
            &format!("const x = {source};"),
        )])
        .expect("parse");
        let ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) =
            &program.files[0].module.body[0]
        else {
            panic!("the item must be a declaration");
        };
        *var.decls[0].init.clone().expect("initializer")
    }

    #[test]
    fn literal_default_type_gives_the_c4_type_of_each_literal_kind() {
        for (source, expected) in [
            ("1", Some(Type::I32)),
            ("0x10", Some(Type::I32)),
            ("-3", Some(Type::I32)),
            ("(7)", Some(Type::I32)),
            ("2.5", Some(Type::F64)),
            ("1e3", Some(Type::F64)),
            ("-(1.5)", Some(Type::F64)),
            ("\"s\"", Some(Type::Str)),
            ("true", Some(Type::Bool)),
            ("false", Some(Type::Bool)),
            ("1 + 1", None),
            ("`s`", None),
            ("-x", None),
        ] {
            assert_eq!(
                literal_default_type(&default_of(source)),
                expected,
                "{source}"
            );
        }
    }
}

#[cfg(test)]
#[path = "erasure_tests.rs"]
mod erasure_tests;
