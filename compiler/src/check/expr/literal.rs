//! Checks literals, template strings, and identifier references.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx, ScopeItem};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, Callee, ExprKind, RegexFn, TplPart};
use crate::types::{FuncType, Type};

use super::{path_key, synthesized_int_range};

impl<'p> Checker<'p> {
    pub(super) fn check_lit(&mut self, lit: &ast::Lit, ctx: Option<&Type>, pos: Pos) -> hir::Expr {
        match lit {
            ast::Lit::Num(n) => self.check_num_lit(n, false, ctx, pos),
            ast::Lit::Str(s) => {
                let value = s.value.to_string();
                if let Some(Type::StringAlias(id)) = ctx {
                    if let Some(discriminant) = self.string_aliases.get(id.0).and_then(|alias| {
                        alias
                            .members
                            .iter()
                            .position(|member| member == &value)
                            .and_then(|index| alias.member_discriminant(index))
                    }) {
                        return hir::Expr {
                            kind: ExprKind::Int(discriminant),
                            ty: Type::StringAlias(*id),
                            pos,
                        };
                    }
                }
                hir::Expr {
                    kind: ExprKind::Str(value),
                    ty: Type::Str,
                    pos,
                }
            }
            ast::Lit::Bool(b) => hir::Expr {
                kind: ExprKind::Bool(b.value),
                ty: Type::Bool,
                pos,
            },
            ast::Lit::Null(_) => hir::Expr {
                kind: ExprKind::Null,
                ty: Type::Null,
                pos,
            },
            ast::Lit::Regex(regex) => {
                let pattern = regex.exp.to_string();
                let flags = regex.flags.to_string();
                if flags.contains('y') {
                    self.error_diverging(
                        RuleCode::S014,
                        "`RegExp.lastIndex` is not in the language: sticky matching requires reading and writing that mutable state (Q31)",
                        pos.clone(),
                        Divergence::RegExpSubset,
                    );
                    return self.err_expr(pos);
                }
                if let Err(error) = crate::regex::validate_literal(&pattern, &flags) {
                    self.error(
                        RuleCode::S100,
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
                        kind: ExprKind::Call {
                            callee: Callee::Regex(RegexFn::New),
                            args: vec![
                                hir::Expr {
                                    kind: ExprKind::Str(pattern),
                                    ty: Type::Str,
                                    pos: pos.clone(),
                                },
                                hir::Expr {
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
                        name: name.clone(),
                        ty: Type::RegExp,
                        mutable: false,
                        init,
                        initializer_index: self.top_level.len(),
                        pos: pos.clone(),
                    });
                    self.regex_literals.insert(key, name.clone());
                    name
                };
                hir::Expr {
                    kind: ExprKind::Global(name),
                    ty: Type::RegExp,
                    pos,
                }
            }
            other => {
                let p = self.pos(other.span());
                self.error(
                    RuleCode::S100,
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
        let hex = raw.starts_with("0x") || raw.starts_with("0X");
        let fractional = raw.contains('.') || (!hex && (raw.contains('e') || raw.contains('E')));
        let value = if negate { -n.value } else { n.value };
        let target = match ctx {
            Some(t) if t.is_numeric() => t.clone(),
            _ => {
                if fractional {
                    Type::F64
                } else {
                    Type::I32
                }
            }
        };
        if target.is_float() {
            // Round-to-nearest-even first overflows binary16 at the
            // midpoint 65520: values below it still round to 65504.
            if target == Type::F16 && value.abs() >= 65_520.0 {
                self.error(
                    RuleCode::S008,
                    format!("numeric literal {} out of range for `f16`", raw),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            return hir::Expr {
                kind: ExprKind::Float(value),
                ty: target,
                pos,
            };
        }
        if fractional {
            let name = self.type_name(&target);
            self.error(
                RuleCode::S008,
                format!("fractional literal in integer context `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let integer = if let Some(raw) = n.raw.as_deref() {
            let (lo, hi) = target
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
            self.error_diverging(
                RuleCode::S008,
                format!("integer literal {} out of range for `{}`", raw, name),
                pos.clone(),
                Divergence::IntegerLiteralRange,
            );
            return self.err_expr(pos);
        };
        hir::Expr {
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
                let checked = self.check_expr(e, None, fx);
                let printable = checked.ty.is_numeric()
                    || matches!(
                        checked.ty,
                        Type::Str | Type::Bool | Type::Enum(_) | Type::StringAlias(_) | Type::Error
                    );
                if checked.ty == Type::Date {
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
                    self.error(
                        RuleCode::S100,
                        format!("type `{}` cannot be interpolated into a template", name),
                        checked.pos.clone(),
                    );
                }
                parts.push(TplPart::Expr(checked));
            }
        }
        hir::Expr {
            kind: ExprKind::Template(parts),
            ty: Type::Str,
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
            self.error_diverging(
                RuleCode::S012,
                "`undefined` is banned; the single null story is `null`",
                pos.clone(),
                if matches!(ctx, Some(Type::StringAlias(_))) {
                    Divergence::OptionalDescriptorMember
                } else {
                    Divergence::GeneralUnionAndUndefined
                },
            );
            return self.err_expr(pos);
        }
        if let Some(local) = self.lookup_local(&name, &pos, fx) {
            let mut expr = hir::Expr {
                kind: ExprKind::Local(name),
                ty: local.ty,
                pos,
            };
            self.apply_narrowing(&mut expr, fx);
            return expr;
        }
        let item = self.scope_item(&name);
        match item {
            Some(ScopeItem::Poisoned) => self.err_expr(pos),
            Some(ScopeItem::Global(g)) => {
                // A mirror flag member (§13.2) folds to its C value here, so
                // both tiers emit an immediate rather than reading a global.
                if let Some((value, ty)) = self.ambient_int_consts.get(&g).cloned() {
                    return hir::Expr {
                        kind: ExprKind::Int(value),
                        ty,
                        pos,
                    };
                }
                let ty = self
                    .global_sigs
                    .get(&g)
                    .map(|s| s.ty.clone())
                    .unwrap_or(Type::Error);
                let mut expr = hir::Expr {
                    kind: ExprKind::Global(g),
                    ty,
                    pos,
                };
                self.apply_narrowing(&mut expr, fx);
                expr
            }
            Some(ScopeItem::Func(f)) => {
                let Some(sig) = self.fn_sigs.get(&f).cloned() else {
                    return self.err_expr(pos);
                };
                if sig.is_generator || sig.is_async {
                    self.error(
                        RuleCode::S100,
                        if sig.is_async {
                            "async functions are not first-class values; call them directly in await position"
                        } else {
                            "generators may only be called, not passed as values"
                        },
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let ty = Type::Func(Box::new(FuncType {
                    params: sig.params.iter().map(|p| p.ty.clone()).collect(),
                    ret: sig.ret,
                }));
                hir::Expr {
                    kind: ExprKind::FuncRef(f),
                    ty,
                    pos,
                }
            }
            Some(ScopeItem::GenericFunc(_)) => {
                self.error(
                    RuleCode::S100,
                    format!(
                        "generic function `{}` requires explicit type arguments",
                        name
                    ),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::Class(_)) | Some(ScopeItem::GenericClass(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("class `{}` used as a value", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::Enum(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("enum `{}` used as a value; use a member", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::StringAlias(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("string-literal union alias `{name}` used as a value"),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            Some(ScopeItem::Foreign(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("foreign function `{}` may only be called", name),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            None => {
                if name == "NaN" {
                    // The ES ambient global is the literal spelling used
                    // by Q24. Local or program declarations named `NaN`
                    // were resolved above and therefore still shadow it.
                    hir::Expr {
                        kind: ExprKind::Float(f64::NAN),
                        ty: Type::F64,
                        pos,
                    }
                } else if name == "eval" || name == "Function" {
                    self.error_diverging(
                        RuleCode::S002,
                        "no dynamic code evaluation",
                        pos.clone(),
                        Divergence::DynamicObjectModel,
                    );
                    self.err_expr(pos)
                } else if name == "Context" {
                    self.error(
                        RuleCode::S014,
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
                    self.error(
                        RuleCode::S014,
                        "`Number` is an ambient namespace, not a value or coercion; \
                         use `Number.<member>` (Q25)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "JSON" {
                    self.error(
                        RuleCode::S014,
                        "`JSON` is an ambient namespace, not a value; use \
                         `JSON.stringify(value)` or `JSON.parse<T>(text)` (Q28)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Date" {
                    // The ambient Date surface is a type and a namespace,
                    // never a value (Q20).
                    self.error(
                        RuleCode::S014,
                        "`Date` is not a value; only `new Date(ms)`, `Date.UTC(…)`, \
                         and `Date.now()` are accepted (Q20)",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Map" || name == "Set" {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`{name}` is a generic reference class, not a value; \
                             construct it with explicit type arguments (Q24)"
                        ),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if crate::ambient::ambient_fn(&name).is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("ambient function `{}` may only be called", name),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if crate::ambient::number_global(&name).is_some() {
                    self.error(
                        RuleCode::S014,
                        format!("`{name}` may only be called, not read as a value (Q25)"),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "isNaN" || name == "isFinite" {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "the coercing global `{name}` is rejected; use `Number.{name}` (Q25)"
                        ),
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else {
                    self.error(
                        RuleCode::S016,
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
        if let Type::Nullable(inner) = &e.ty {
            if let Some(key) = path_key(e) {
                if fx.narrowed.contains(&key) {
                    e.ty = (**inner).clone();
                }
            }
        }
    }
}
