//! Expression checking: contextual literal typing (C4), sized-numeric
//! arithmetic (C3/Q18), nominal member access (C1/Q4/Q5), calls, `as`
//! conversions, lambdas (C5), and null narrowing at use sites (C7).

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Pos, RuleCode};
use crate::hir::{
    self, AmbientFn, ArrFn, AsyncCallee, BinOp, Callee, ContextBytesFn, DateFn, ExprKind, MapFn,
    MathFn, NumFn, RegexFn, SetFn, StrFn, TplPart, UnOp, WorkerFn,
};
use crate::types::{ClassId, FuncType, Type};

use super::stmt::narrow_paths;
use super::{Checker, FnCtx, Frame, Local, ParamSig, Scope, ScopeItem};

/// Dotted path key for narrowing (`node`, `node.next`, `this.x`).
pub(crate) fn path_key(e: &hir::Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Local(n) | ExprKind::Global(n) => Some(n.clone()),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Field { obj, name } => path_key(obj).map(|p| format!("{}.{}", p, name)),
        ExprKind::JsonResultValue(obj) => path_key(obj).map(|p| format!("{p}.value")),
        _ => None,
    }
}

/// True for literals that can adopt a contextual type: numeric literals
/// (C4) and string literals in a Q32 alias context.
fn literalish(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Lit(ast::Lit::Num(_) | ast::Lit::Str(_)) => true,
        ast::Expr::Paren(p) => literalish(&p.expr),
        ast::Expr::Unary(u) if u.op == ast::UnaryOp::Minus => literalish(&u.arg),
        _ => false,
    }
}

/// Recognizes the one token whose ordinary identifier path is banned by C7.
/// R16 handles it before general expression checking only for strict
/// comparisons against an absence-capable descriptor member.
fn is_undefined_ident(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Ident(id) => id.sym.as_ref() == "undefined",
        ast::Expr::Paren(paren) => is_undefined_ident(&paren.expr),
        _ => false,
    }
}

fn unparen_expr(mut e: &ast::Expr) -> &ast::Expr {
    while let ast::Expr::Paren(paren) = e {
        e = &paren.expr;
    }
    e
}

/// Returns the nominal class supplied by an object literal's context.
/// Q33/R17 permits descriptor construction through either `D` or `D | null`;
/// retaining plain classes here also preserves their specific S005 rejection.
fn contextual_object_class(ctx: Option<&Type>) -> Option<ClassId> {
    match ctx? {
        Type::Class(id) => Some(*id),
        Type::Nullable(inner) => match inner.as_ref() {
            Type::Class(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum DescriptorProp<'a> {
    Expr(&'a ast::Expr),
    Shorthand(&'a ast::Ident),
}

fn regex_literal(e: &ast::Expr) -> Option<&ast::Regex> {
    match e {
        ast::Expr::Lit(ast::Lit::Regex(regex)) => Some(regex),
        ast::Expr::Paren(paren) => regex_literal(&paren.expr),
        _ => None,
    }
}

fn int_range(ty: &Type) -> Option<(i128, i128)> {
    match ty {
        Type::I8 => Some((i128::from(i8::MIN), i128::from(i8::MAX))),
        Type::U8 => Some((0, i128::from(u8::MAX))),
        Type::I16 => Some((i128::from(i16::MIN), i128::from(i16::MAX))),
        Type::U16 => Some((0, i128::from(u16::MAX))),
        Type::I32 => Some((i128::from(i32::MIN), i128::from(i32::MAX))),
        Type::U32 => Some((0, i128::from(u32::MAX))),
        Type::I64 => Some((i128::from(i64::MIN), i128::from(i64::MAX))),
        Type::U64 => Some((0, i128::from(u64::MAX))),
        _ => None,
    }
}

/// The pre-R26 f64 range retained for synthesized numeric nodes without a
/// source spelling. Such nodes are exact within this channel's old cap.
fn synthesized_int_range(ty: &Type) -> Option<(i64, i64)> {
    const EXACT: i64 = 9_007_199_254_740_991;
    match ty {
        Type::I8 => Some((i64::from(i8::MIN), i64::from(i8::MAX))),
        Type::U8 => Some((0, i64::from(u8::MAX))),
        Type::I16 => Some((i64::from(i16::MIN), i64::from(i16::MAX))),
        Type::U16 => Some((0, i64::from(u16::MAX))),
        Type::I32 => Some((i64::from(i32::MIN), i64::from(i32::MAX))),
        Type::U32 => Some((0, i64::from(u32::MAX))),
        Type::I64 => Some((-EXACT, EXACT)),
        Type::U64 => Some((0, EXACT)),
        _ => None,
    }
}

/// Reinterprets HIR integer bits according to the expression's sized type.
fn int_value_at_type(bits: i64, ty: &Type) -> Option<i128> {
    Some(match ty {
        Type::I8 => i128::from(bits as i8),
        Type::U8 => i128::from(bits as u8),
        Type::I16 => i128::from(bits as i16),
        Type::U16 => i128::from(bits as u16),
        Type::I32 => i128::from(bits as i32),
        Type::U32 => i128::from(bits as u32),
        Type::I64 => i128::from(bits),
        Type::U64 => i128::from(bits as u64),
        _ => return None,
    })
}

fn integer_width(ty: &Type) -> Option<i64> {
    Some(match ty {
        Type::I8 | Type::U8 => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 => 32,
        Type::I64 | Type::U64 => 64,
        _ => return None,
    })
}

impl<'p> Checker<'p> {
    /// Emits a checker-owned generated-reference rejection.
    fn reject_api_form(&mut self, group: &str, surface: &str, actual: &str, pos: Pos) -> bool {
        let Some(rejection) = crate::ambient::form_rejection(group, surface) else {
            return false;
        };
        self.error(
            rejection.code,
            crate::ambient::rejection_message(rejection, actual),
            pos,
        );
        true
    }

    pub(crate) fn err_expr(&self, pos: Pos) -> hir::Expr {
        hir::Expr {
            kind: ExprKind::Null,
            ty: Type::Error,
            pos,
        }
    }

    /// Checks one expression. `ctx` is the contextual type used to type
    /// suffix-less numeric literals (C4); it never coerces non-literals.
    pub(crate) fn check_expr(
        &mut self,
        e: &ast::Expr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let pos = self.pos(e.span());
        match e {
            ast::Expr::Paren(p) => self.check_expr(&p.expr, ctx, fx),
            ast::Expr::Lit(lit) => self.check_lit(lit, ctx, pos),
            ast::Expr::Tpl(tpl) => self.check_template(tpl, fx, pos),
            ast::Expr::Ident(id) => self.check_ident(id, fx),
            ast::Expr::This(_) => {
                let this_ty = fx.frames.last().and_then(|f| f.this_ty.clone());
                match this_ty {
                    Some(ty) => hir::Expr {
                        kind: ExprKind::This,
                        ty,
                        pos,
                    },
                    None => {
                        self.error(
                            RuleCode::S100,
                            "`this` is only available in constructors and methods",
                            pos.clone(),
                        );
                        self.err_expr(pos)
                    }
                }
            }
            ast::Expr::Unary(u) => self.check_unary(u, ctx, fx, pos),
            ast::Expr::Update(u) => self.check_update(u, fx, pos),
            ast::Expr::Bin(b) => self.check_bin(b, ctx, fx, pos),
            ast::Expr::Assign(a) => self.check_assign(a, fx, pos, false),
            ast::Expr::Member(m) => self.check_member_read(m, fx),
            ast::Expr::Cond(c) => self.check_cond(c, ctx, fx, pos),
            ast::Expr::Call(c) => self.check_call(c, ctx, fx, pos),
            ast::Expr::New(n) => self.check_new(n, fx, pos),
            ast::Expr::Arrow(a) => self.check_lambda(a, ctx, fx, pos),
            ast::Expr::Array(a) => self.check_array_lit(a, ctx, fx, pos),
            ast::Expr::Object(object) => match contextual_object_class(ctx) {
                Some(id) if self.classes[id.0].is_descriptor => {
                    self.check_descriptor_lit(object, id, fx, pos)
                }
                Some(_) => {
                    self.error(
                        RuleCode::S005,
                        "object literals do not satisfy nominal class types",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                }
                _ => {
                    self.error(
                        RuleCode::S100,
                        "object literals are not in the decided surface",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                }
            },
            ast::Expr::TsAs(a) => self.check_as(a, fx, pos),
            ast::Expr::Yield(y) => self.check_yield(y, fx, pos),
            ast::Expr::Await(a) => self.check_await(a, fx, pos),
            ast::Expr::TsNonNull(t) => {
                let p = self.pos(t.span);
                self.error(
                    RuleCode::S100,
                    "the `!` assertion is not in the decided surface; narrow with a null check",
                    p.clone(),
                );
                self.err_expr(p)
            }
            ast::Expr::Fn(_) => {
                self.error(
                    RuleCode::S100,
                    "function expressions are not in the decided surface; use an arrow",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            other => {
                let p = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "expression form outside the decided surface",
                    p.clone(),
                );
                self.err_expr(p)
            }
        }
    }

    /// Checks an expression statement, admitting the ambient
    /// `unreachable()` only when it is the statement's direct call.
    pub(crate) fn check_expr_stmt(&mut self, e: &ast::Expr, fx: &mut FnCtx) -> hir::Expr {
        let mut root = e;
        while let ast::Expr::Paren(paren) = root {
            root = &paren.expr;
        }
        if let ast::Expr::Call(call) = root {
            if let ast::Callee::Expr(callee) = &call.callee {
                let mut callee: &ast::Expr = callee;
                while let ast::Expr::Paren(paren) = callee {
                    callee = &paren.expr;
                }
                if let ast::Expr::Ident(ident) = callee {
                    if ident.sym.as_ref() == "unreachable" {
                        let pos = self.pos(call.span);
                        return self.check_named_call(ident, call, fx, pos, true);
                    }
                }
            }
        }
        if let ast::Expr::Assign(assign) = root {
            return self.check_assign(assign, fx, self.pos(root.span()), true);
        }
        self.check_expr(e, None, fx)
    }

    /// Checks Q34/R13's three awaitable forms. The AST call is handled here
    /// instead of through the ordinary call path so an async call can never
    /// materialize a Promise-typed value in HIR.
    fn check_await(&mut self, awaited: &ast::AwaitExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        if !fx.frames.last().is_some_and(|frame| frame.is_async) {
            self.error(
                RuleCode::S013,
                "`await` is only legal inside an async function",
                pos.clone(),
            );
            return self.err_expr(pos);
        }

        let mut operand: &ast::Expr = &awaited.arg;
        while let ast::Expr::Paren(paren) = operand {
            operand = &paren.expr;
        }
        let ast::Expr::Call(call) = operand else {
            self.error(
                RuleCode::S100,
                "awaitable expressions are exactly `Context.suspend()` and direct async function or method calls",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let ast::Callee::Expr(callee) = &call.callee else {
            self.error(
                RuleCode::S100,
                "awaitable expressions must be direct calls",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let mut callee: &ast::Expr = callee;
        while let ast::Expr::Paren(paren) = callee {
            callee = &paren.expr;
        }

        if let ast::Expr::Member(member) = callee {
            if self.is_context_namespace(&member.obj, fx)
                && matches!(&member.prop, ast::MemberProp::Ident(prop) if prop.sym.as_ref() == "suspend")
            {
                if call.type_args.is_some() || !call.args.is_empty() {
                    self.error(
                        RuleCode::S100,
                        "`Context.suspend()` takes no type arguments or value arguments",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                return hir::Expr {
                    kind: ExprKind::AsyncSuspend,
                    ty: Type::Void,
                    pos,
                };
            }
        }

        match callee {
            ast::Expr::Ident(ident) => {
                let name = ident.sym.to_string();
                if fx
                    .scopes
                    .iter()
                    .rev()
                    .any(|scope| scope.vars.contains_key(&name))
                {
                    self.error(
                        RuleCode::S100,
                        "an async awaitable cannot be called through a local value",
                        self.pos(ident.span),
                    );
                    return self.err_expr(pos);
                }
                let Some(ScopeItem::Func(function)) = self.scope_item(&name) else {
                    self.error(
                        RuleCode::S100,
                        format!("`{name}` is not a directly declared async function"),
                        self.pos(ident.span),
                    );
                    return self.err_expr(pos);
                };
                let Some(sig) = self.fn_sigs.get(&function).cloned() else {
                    return self.err_expr(pos);
                };
                if !sig.is_async {
                    self.error(
                        RuleCode::S100,
                        format!("`{name}` is synchronous and cannot be awaited"),
                        self.pos(ident.span),
                    );
                    return self.err_expr(pos);
                }
                if call.type_args.is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("`{name}` is not generic"),
                        self.pos(ident.span),
                    );
                }
                let args = self.check_args(&sig.params, &call.args, fx, &pos, &name);
                hir::Expr {
                    kind: ExprKind::AsyncCall {
                        callee: AsyncCallee::Function(function),
                        args,
                    },
                    ty: sig.ret,
                    pos,
                }
            }
            ast::Expr::Member(member) => {
                let ast::MemberProp::Ident(method) = &member.prop else {
                    self.error(
                        RuleCode::S100,
                        "an awaited async method requires an identifier method name",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                let name = method.sym.to_string();
                let method_pos = self.pos(method.span);
                let receiver = self.check_receiver(&member.obj, fx);
                let Type::Class(class) = receiver.ty.clone() else {
                    if receiver.ty != Type::Error {
                        let receiver_ty = self.type_name(&receiver.ty);
                        self.error(
                            RuleCode::S100,
                            format!("type `{receiver_ty}` has no async method `{name}`"),
                            method_pos,
                        );
                    }
                    return self.err_expr(pos);
                };
                let Some(sig) = self.class_sigs[class.0].methods.get(&name).cloned() else {
                    let class_name = self.classes[class.0].name.clone();
                    self.error(
                        RuleCode::S100,
                        format!("`{class_name}` has no method `{name}`"),
                        method_pos,
                    );
                    return self.err_expr(pos);
                };
                if !sig.is_async {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` is synchronous and cannot be awaited"),
                        method_pos,
                    );
                    return self.err_expr(pos);
                }
                if call.type_args.is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` is not generic"),
                        method_pos,
                    );
                }
                let args = self.check_args(&sig.params, &call.args, fx, &pos, &name);
                hir::Expr {
                    kind: ExprKind::AsyncCall {
                        callee: AsyncCallee::Method {
                            class,
                            receiver: Box::new(receiver),
                            name,
                        },
                        args,
                    },
                    ty: sig.ret,
                    pos,
                }
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    "an async awaitable must directly call a named async function or instance method",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    fn check_lit(&mut self, lit: &ast::Lit, ctx: Option<&Type>, pos: Pos) -> hir::Expr {
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
                    self.error(
                        RuleCode::S014,
                        "`RegExp.lastIndex` is not in the language: sticky matching requires reading and writing that mutable state (Q31)",
                        pos.clone(),
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
    fn check_num_lit(
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
            let (lo, hi) =
                int_range(&target).unwrap_or((i128::from(i64::MIN), i128::from(i64::MAX)));
            super::parse_integer_spelling(raw, negate)
                .filter(|value| *value >= lo && *value <= hi)
                .map(|value| value as i64)
        } else {
            // Synthesized numeric nodes have no source spelling; retain the
            // parser-value path used before R26.
            let (lo, hi) = synthesized_int_range(&target).unwrap_or((i64::MIN, i64::MAX));
            (value >= lo as f64 && value <= hi as f64).then_some(value as i64)
        };
        let Some(integer) = integer else {
            let name = self.type_name(&target);
            self.error(
                RuleCode::S008,
                format!("integer literal {} out of range for `{}`", raw, name),
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        hir::Expr {
            kind: ExprKind::Int(integer),
            ty: target,
            pos,
        }
    }

    fn check_template(&mut self, tpl: &ast::Tpl, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
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

    fn check_ident(&mut self, id: &ast::Ident, fx: &mut FnCtx) -> hir::Expr {
        let name = id.sym.to_string();
        let pos = self.pos(id.span);
        if name == "undefined" {
            self.error(
                RuleCode::S012,
                "`undefined` is banned; the single null story is `null`",
                pos.clone(),
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
        match self.scope_item(&name) {
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
                    self.error(RuleCode::S002, "no dynamic code evaluation", pos.clone());
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
                        RuleCode::S100,
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
    fn apply_narrowing(&self, e: &mut hir::Expr, fx: &FnCtx) {
        if let Type::Nullable(inner) = &e.ty {
            if let Some(key) = path_key(e) {
                if fx.narrowed.contains(&key) {
                    e.ty = (**inner).clone();
                }
            }
        }
    }

    fn check_unary(
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
                    self.error(
                        RuleCode::S014,
                        "arithmetic on `f16` is not supported; compute via `as f32`",
                        pos.clone(),
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

    fn check_update(&mut self, u: &ast::UpdateExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let target = self.check_expr(&u.arg, None, fx);
        if matches!(
            &target.kind,
            ExprKind::Call {
                callee: Callee::Method { recv, name },
                args,
            } if name == "get"
                && args.len() == 1
                && matches!(&recv.ty, Type::Class(id) if self.classes[id.0].index_signature.is_some())
        ) {
            let operator = if u.op == ast::UpdateOp::PlusPlus {
                "++"
            } else {
                "--"
            };
            let spelling = if u.prefix {
                format!("`{operator}a[i]`")
            } else {
                format!("`a[i]{operator}`")
            };
            self.error(
                RuleCode::S100,
                format!("{spelling} is not supported for a class index signature"),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        // Q17: `++`/`--` rebind their target, so `const` bindings are
        // rejected exactly like plain assignment.
        match &target.kind {
            ExprKind::Local(name) => {
                let mutable = fx
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|s| s.vars.get(name))
                    .map(|l| l.mutable)
                    .unwrap_or(true);
                if !mutable {
                    self.error(
                        RuleCode::S100,
                        format!("cannot rebind `const` binding `{}`", name),
                        target.pos.clone(),
                    );
                }
            }
            ExprKind::Global(name) => {
                if let Some(sig) = self.global_sigs.get(name) {
                    if !sig.mutable {
                        let name = name.clone();
                        self.error(
                            RuleCode::S100,
                            format!("cannot rebind `const` binding `{}`", name),
                            target.pos.clone(),
                        );
                    }
                }
            }
            _ => {}
        }
        if !target.ty.is_numeric() && !matches!(target.ty, Type::Error) {
            let name = self.type_name(&target.ty);
            self.error(
                RuleCode::S100,
                format!("`++`/`--` require a numeric target, got `{}`", name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if target.ty == Type::F16 {
            self.error(
                RuleCode::S014,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let op = if u.op == ast::UpdateOp::PlusPlus {
            BinOp::Add
        } else {
            BinOp::Sub
        };
        let ty = target.ty.clone();
        let one = hir::Expr {
            kind: if ty.is_float() {
                ExprKind::Float(1.0)
            } else {
                ExprKind::Int(1)
            },
            ty: ty.clone(),
            pos: pos.clone(),
        };
        hir::Expr {
            kind: ExprKind::Assign {
                op: Some(op),
                target: Box::new(target),
                value: Box::new(one),
            },
            ty,
            pos,
        }
    }

    fn check_bin(
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
            B::In | B::InstanceOf | B::Exp | B::NullishCoalescing => {
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
                self.bin_result(b.op, left, right, pos)
            }
        }
    }

    /// Checks R16's sole legal `undefined` appearance. The source token is
    /// erased to the reserved i32 discriminant in HIR, so both backends use
    /// their ordinary integer comparison lowering.
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

        let sentinel_value = match &checked.ty {
            Type::StringAlias(id) => self
                .string_aliases
                .get(id.0)
                .map_or(-1, hir::StringAliasDef::absence_discriminant),
            _ => -1,
        };
        let sentinel = hir::Expr {
            kind: ExprKind::Int(sentinel_value),
            ty: checked.ty.clone(),
            pos: self.pos(undefined_source.span()),
        };
        let (left, right) = if left_undefined {
            (sentinel, checked)
        } else {
            (checked, sentinel)
        };
        Some(hir::Expr {
            kind: ExprKind::Binary {
                op: if binary.op == ast::BinaryOp::EqEqEq {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                },
                left: Box::new(left),
                right: Box::new(right),
            },
            ty: Type::Bool,
            pos,
        })
    }

    fn bin_result(
        &mut self,
        op: ast::BinaryOp,
        left: hir::Expr,
        right: hir::Expr,
        pos: Pos,
    ) -> hir::Expr {
        use ast::BinaryOp as B;
        let lt = left.ty.clone();
        let rt = right.ty.clone();
        let err = matches!(lt, Type::Error) || matches!(rt, Type::Error);
        let mixed_numeric = lt.is_numeric() && rt.is_numeric() && lt != rt;
        let arithmetic = matches!(op, B::Add | B::Sub | B::Mul | B::Div | B::Mod);
        if arithmetic && (lt == Type::F16 || rt == Type::F16) {
            self.error(
                RuleCode::S014,
                "arithmetic on `f16` is not supported; compute via `as f32`",
                pos.clone(),
            );
            return self.err_expr(pos);
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
                if lt == Type::Str && rt == Type::Str {
                    (BinOp::Add, Type::Str, true)
                } else if lt.is_numeric() && lt == rt {
                    (BinOp::Add, lt.clone(), true)
                } else {
                    (BinOp::Add, Type::Error, err)
                }
            }
            B::Sub | B::Mul | B::Div | B::Mod => {
                let hop = match op {
                    B::Sub => BinOp::Sub,
                    B::Mul => BinOp::Mul,
                    B::Div => BinOp::Div,
                    _ => BinOp::Rem,
                };
                if lt.is_numeric() && lt == rt {
                    (hop, lt.clone(), true)
                } else {
                    (hop, Type::Error, err)
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
                } else if lt.is_integer() && rt.is_integer() {
                    // Q18: mixed-width bitwise requires `as`.
                    (hop, Type::Error, err)
                } else {
                    (hop, Type::Error, err)
                }
            }
            _ => (BinOp::Add, Type::Error, err),
        };
        let literal_shift_amount = match &right.kind {
            ExprKind::Int(bits) => int_value_at_type(*bits, &right.ty),
            _ => None,
        };
        if ok
            && matches!(op, B::LShift | B::RShift | B::ZeroFillRShift)
            && literal_shift_amount
                .is_some_and(|amount| amount >= i128::from(integer_width(&lt).unwrap_or(i64::MAX)))
        {
            let amount = literal_shift_amount.unwrap_or(0);
            let width = integer_width(&lt).unwrap_or(0);
            let name = self.type_name(&lt);
            self.error(
                RuleCode::S008,
                format!(
                    "literal shift amount {} is out of range for `{}` width {}",
                    amount, name, width
                ),
                right.pos.clone(),
            );
            return self.err_expr(pos);
        }
        if ok || err {
            return mk(hop, if err { Type::Error } else { ty });
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
            return self.err_expr(pos);
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
            self.error(
                RuleCode::S007,
                format!(
                    "mixed-type {} (`{}` and `{}`) requires an explicit `as` conversion",
                    family, ln, rn
                ),
                pos.clone(),
            );
        } else {
            self.error(
                RuleCode::S100,
                format!("operator not defined for `{}` and `{}`", ln, rn),
                pos.clone(),
            );
        }
        self.err_expr(pos)
    }

    fn check_cond(
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
        let (then_extra, else_extra) = narrow_paths(&cond, &|id| {
            self.string_aliases[id.0].absence_discriminant()
        });
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
            self.require_assignable(
                &els.ty.clone(),
                &then_ty,
                els.pos.clone(),
                "the else branch",
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

    fn check_yield(&mut self, y: &ast::YieldExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
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

    fn check_as(&mut self, a: &ast::TsAsExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
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

    fn check_array_lit(
        &mut self,
        a: &ast::ArrayLit,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if a.elems
            .iter()
            .flatten()
            .any(|element| element.spread.is_some())
        {
            return self.check_array_spread_lit(a, ctx, fx, pos);
        }
        let mut elems: Vec<&ast::ExprOrSpread> = Vec::new();
        for e in &a.elems {
            match e {
                Some(e) if e.spread.is_none() => elems.push(e),
                Some(_) => unreachable!("spread literal dispatched above"),
                None => {
                    self.error(RuleCode::S100, "array holes are not decided", pos.clone());
                }
            }
        }
        match ctx {
            Some(Type::Array(elem_ty)) => {
                let elem_ty = (**elem_ty).clone();
                let mut out = Vec::new();
                for e in elems {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &elem_ty,
                        checked.pos.clone(),
                        "the array element",
                    );
                    if self.is_capturing_value(&checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not be stored in arrays",
                            checked.pos.clone(),
                        );
                    }
                    out.push(checked);
                }
                hir::Expr {
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::Array(Box::new(elem_ty)),
                    pos,
                }
            }
            Some(Type::FixedArray(elem_ty, n)) => {
                let elem_ty = (**elem_ty).clone();
                let n = *n;
                if elems.len() != n as usize {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "FixedArray length mismatch: the annotation says {}, \
                             the literal has {} elements",
                            n,
                            elems.len()
                        ),
                        pos.clone(),
                    );
                }
                let mut out = Vec::new();
                for e in elems {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &elem_ty,
                        checked.pos.clone(),
                        "the array element",
                    );
                    if self.is_capturing_value(&checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not be stored in arrays",
                            checked.pos.clone(),
                        );
                    }
                    out.push(checked);
                }
                hir::Expr {
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::FixedArray(Box::new(elem_ty), n),
                    pos,
                }
            }
            _ => {
                if elems.is_empty() {
                    self.error(
                        RuleCode::S100,
                        "cannot infer the type of an empty array literal without context",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let first = self.check_expr(&elems[0].expr, None, fx);
                let elem_ty = first.ty.clone();
                if Self::is_context_affine_type(&elem_ty) {
                    self.error(
                        RuleCode::S100,
                        "Worker, Inbox, and Outbox values may not be array elements",
                        first.pos.clone(),
                    );
                }
                let mut out = vec![first];
                for e in &elems[1..] {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &elem_ty,
                        checked.pos.clone(),
                        "the array element",
                    );
                    out.push(checked);
                }
                for checked in &out {
                    if self.is_capturing_value(checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not be stored in arrays",
                            checked.pos.clone(),
                        );
                    }
                }
                hir::Expr {
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::Array(Box::new(elem_ty)),
                    pos,
                }
            }
        }
    }

    fn check_descriptor_lit(
        &mut self,
        object: &ast::ObjectLit,
        class_id: crate::types::ClassId,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let class = self.classes[class_id.0].clone();
        let mut provided: Vec<(String, DescriptorProp<'_>, Pos)> = Vec::new();
        for prop in &object.props {
            let (name, value, prop_pos) = match prop {
                ast::PropOrSpread::Spread(spread) => {
                    self.error(
                        RuleCode::S100,
                        "spread properties are not supported in descriptor literals",
                        self.pos(spread.dot3_token),
                    );
                    continue;
                }
                ast::PropOrSpread::Prop(prop) => match &**prop {
                    ast::Prop::KeyValue(key_value) => {
                        let ast::PropName::Ident(key) = &key_value.key else {
                            self.error(
                                RuleCode::S100,
                                "descriptor literal member names must be identifiers",
                                self.pos(key_value.key.span()),
                            );
                            continue;
                        };
                        (
                            key.sym.to_string(),
                            DescriptorProp::Expr(&key_value.value),
                            self.pos(key.span),
                        )
                    }
                    ast::Prop::Shorthand(ident) => (
                        ident.sym.to_string(),
                        DescriptorProp::Shorthand(ident),
                        self.pos(ident.span),
                    ),
                    other => {
                        self.error(
                            RuleCode::S100,
                            "descriptor literals contain data properties only",
                            self.pos(other.span()),
                        );
                        continue;
                    }
                },
            };
            if provided.iter().any(|(existing, _, _)| existing == &name) {
                self.error(
                    RuleCode::S100,
                    format!("duplicate descriptor literal member `{name}`"),
                    prop_pos,
                );
                continue;
            }
            if !class.fields.iter().any(|field| field.name == name) {
                self.error(
                    RuleCode::S004,
                    format!(
                        "descriptor class `{}` has no declared property `{name}`",
                        class.name
                    ),
                    prop_pos,
                );
                continue;
            }
            provided.push((name, value, prop_pos));
        }

        let mut fields = Vec::with_capacity(class.fields.len());
        for field in &class.fields {
            let explicit = provided
                .iter()
                .find(|(name, _, _)| name == &field.name)
                .map(|(_, value, _)| *value);
            let checked = match explicit {
                Some(DescriptorProp::Expr(value)) => {
                    Some(self.check_expr(value, Some(&field.ty), fx))
                }
                Some(DescriptorProp::Shorthand(ident)) => Some(self.check_ident(ident, fx)),
                None if field.is_defaulted => None,
                None if field.is_absence_capable => {
                    let sentinel = match &field.ty {
                        Type::StringAlias(id) => self
                            .string_aliases
                            .get(id.0)
                            .map_or(-1, hir::StringAliasDef::absence_discriminant),
                        _ => -1,
                    };
                    Some(hir::Expr {
                        kind: ExprKind::Int(sentinel),
                        ty: field.ty.clone(),
                        pos: pos.clone(),
                    })
                }
                None => {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "descriptor literal for `{}` is missing required member `{}`",
                            class.name, field.name
                        ),
                        pos.clone(),
                    );
                    None
                }
            };
            if let Some(checked) = &checked {
                self.require_assignable(
                    &checked.ty.clone(),
                    &field.ty,
                    checked.pos.clone(),
                    "the descriptor member",
                );
                if self.is_capturing_value(checked, fx) {
                    self.error(
                        RuleCode::S009,
                        "capturing lambdas may not escape into descriptor objects",
                        checked.pos.clone(),
                    );
                }
            }
            fields.push(checked);
        }

        hir::Expr {
            kind: ExprKind::DescriptorLit {
                class: class_id,
                fields,
            },
            ty: Type::Class(class_id),
            pos,
        }
    }

    /// Checks an array literal containing spread. P22 keeps this as a
    /// distinct HIR form so ordinary literals and `FixedArray` in-place
    /// construction retain their existing lowering unchanged.
    fn check_array_spread_lit(
        &mut self,
        a: &ast::ArrayLit,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if matches!(ctx, Some(Type::FixedArray(..))) {
            self.error(
                RuleCode::S014,
                "array-literal spread produces a fresh T[]; it cannot construct a FixedArray",
                pos.clone(),
            );
        }
        let context_elem = match ctx {
            Some(Type::Array(elem)) => Some((**elem).clone()),
            _ => None,
        };
        let mut checked = Vec::new();
        let mut inferred: Option<Type> = context_elem.clone();
        for slot in &a.elems {
            let Some(slot) = slot else {
                self.error(RuleCode::S100, "array holes are not decided", pos.clone());
                continue;
            };
            let is_spread = slot.spread.is_some();
            let expr = self.check_expr(
                &slot.expr,
                if is_spread { None } else { inferred.as_ref() },
                fx,
            );
            let (spread, element_ty) = if is_spread {
                let selected = match &expr.ty {
                    Type::Array(elem) => Some((hir::SpreadKind::Array, (**elem).clone())),
                    Type::FixedArray(elem, _) => {
                        Some((hir::SpreadKind::FixedArray, (**elem).clone()))
                    }
                    Type::Map(key, _) => Some((hir::SpreadKind::MapKeys, (**key).clone())),
                    Type::Set(key) => Some((hir::SpreadKind::SetValues, (**key).clone())),
                    Type::Str => Some((hir::SpreadKind::StringCodePoints, Type::Str)),
                    Type::Generator(_) => {
                        self.error(
                            RuleCode::S014,
                            "Generator<T> is single-use; array-literal spread would consume \
                             a value expression",
                            self.pos(slot.spread.unwrap_or(a.span)),
                        );
                        None
                    }
                    Type::Error => None,
                    other => {
                        let actual = self.type_name(other);
                        self.error(
                            RuleCode::S014,
                            format!(
                                "array-literal spread accepts T[], FixedArray<T, N>, Map, \
                                 Set, or string; got `{actual}`"
                            ),
                            self.pos(slot.spread.unwrap_or(a.span)),
                        );
                        None
                    }
                };
                match selected {
                    Some((kind, ty)) => (Some(kind), ty),
                    None => (None, Type::Error),
                }
            } else {
                (None, expr.ty.clone())
            };
            if inferred.is_none() && !matches!(element_ty, Type::Error) {
                inferred = Some(element_ty.clone());
            }
            if let Some(expected) = &inferred {
                self.require_assignable(
                    &element_ty,
                    expected,
                    expr.pos.clone(),
                    "the array element",
                );
            }
            if !is_spread && self.is_capturing_value(&expr, fx) {
                self.error(
                    RuleCode::S009,
                    "capturing lambdas may not be stored in arrays",
                    expr.pos.clone(),
                );
            }
            checked.push(hir::ArrayLitElem { expr, spread });
        }
        let Some(elem_ty) = inferred else {
            self.error(
                RuleCode::S100,
                "cannot infer the type of an empty array literal without context",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        if Self::is_context_affine_type(&elem_ty) {
            self.error(
                RuleCode::S100,
                "Worker, Inbox, and Outbox values may not be array elements",
                pos.clone(),
            );
        }
        hir::Expr {
            kind: ExprKind::ArraySpreadLit(checked),
            ty: Type::Array(Box::new(elem_ty)),
            pos,
        }
    }

    // ----- member access -----

    /// Checks a receiver expression and enforces C7: member access on a
    /// nullable value requires prior narrowing.
    fn check_receiver(&mut self, obj: &ast::Expr, fx: &mut FnCtx) -> hir::Expr {
        let mut checked = self.check_expr(obj, None, fx);
        self.apply_narrowing(&mut checked, fx);
        if let Type::Nullable(_) = checked.ty {
            let name = self.type_name(&checked.ty);
            self.error(
                RuleCode::S011,
                format!(
                    "`{}` may be null here; narrow with a null check first",
                    name
                ),
                checked.pos.clone(),
            );
            checked.ty = Type::Error;
        }
        checked
    }

    /// Resolves `obj` in `obj.prop` when `obj` is a type name used as a
    /// namespace (enums, class statics, `Object.setPrototypeOf`, the
    /// ambient `Math` of stdlib.md §1). Returns `Some` when handled.
    /// `for_write` marks an assignment-target position.
    fn check_namespace_member(
        &mut self,
        obj: &ast::Expr,
        prop: &str,
        prop_pos: Pos,
        fx: &mut FnCtx,
        for_write: bool,
    ) -> Option<hir::Expr> {
        let ast::Expr::Ident(id) = obj else {
            return None;
        };
        let name = id.sym.to_string();
        // A local binding shadows any type name.
        let is_local = fx.scopes.iter().rev().any(|s| s.vars.contains_key(&name));
        if is_local {
            return None;
        }
        // `Context.<member>` (Q6/Q7/Q34): function members are intercepted
        // in call position; the namespace and its members are not values.
        if name == "Context" && self.scope_item(&name).is_none() {
            let detail = if prop == "suspend" {
                "`Context.suspend` may only appear as the direct call in `await Context.suspend()` (Q34)".to_string()
            } else if crate::ambient::context_fn(prop).is_some()
                || crate::ambient::context_bytes_fn(prop).is_some()
            {
                format!("`Context.{prop}` may only be called, not read as a value (Q6/Q7/Q34)")
            } else {
                format!("`Context.{prop}` is outside the accepted Context subset (Q6/Q7/Q34)")
            };
            self.error(RuleCode::S014, detail, prop_pos.clone());
            return Some(self.err_expr(prop_pos));
        }
        // `Math.<member>` (stdlib.md §1): the ambient namespace applies
        // only when no program declaration shadows the name. Function
        // members are intercepted by `check_method_call` before this
        // point; here a member is a constant fold, a rejected
        // un-called function, or an out-of-subset rejection.
        if name == "Math" && self.scope_item(&name).is_none() {
            return Some(self.check_math_member(prop, prop_pos, for_write));
        }
        // `Number.<member>` (stdlib.md §11, Q25): predicate calls are
        // intercepted by `check_method_call`; here constants fold and
        // every other read/write receives the subset diagnostic.
        if name == "Number" && self.scope_item(&name).is_none() {
            return Some(self.check_number_member(prop, prop_pos, for_write));
        }
        // `JSON.<member>` (stdlib.md §13, Q28): accepted functions are
        // intercepted in call position; namespace members are not values.
        if name == "JSON" && self.scope_item(&name).is_none() {
            let detail = if matches!(prop, "stringify" | "parse") {
                format!("`JSON.{prop}` may only be called, not read as a value (Q28)")
            } else {
                format!("`JSON.{prop}` is outside the accepted JSON subset (Q28)")
            };
            self.error(RuleCode::S014, detail, prop_pos.clone());
            return Some(self.err_expr(prop_pos));
        }
        // `Date.<member>` (stdlib.md §3): the static function members
        // (`UTC`, `now`) are intercepted by `check_method_call` before
        // this point; here every member read is a rejection.
        if name == "Date" && self.scope_item(&name).is_none() {
            return Some(self.check_date_member(prop, prop_pos, for_write));
        }
        if (name == "Map" || name == "Set") && self.scope_item(&name).is_none() {
            if name == "Map" && prop == "groupBy" {
                self.error(
                    RuleCode::S014,
                    "`Map.groupBy` may only be called, not read as a value (Q27)",
                    prop_pos.clone(),
                );
                return Some(self.err_expr(prop_pos));
            }
            self.error(
                RuleCode::S014,
                format!(
                    "`{name}.{prop}` is outside the accepted Map/Set subset; \
                     iterator-based APIs are rejected (Q24)"
                ),
                prop_pos.clone(),
            );
            return Some(self.err_expr(prop_pos));
        }
        match self.scope_item(&name) {
            Some(ScopeItem::Class(_)) | Some(ScopeItem::GenericClass(_)) => {
                if prop == "prototype" {
                    self.error(RuleCode::S003, "no prototype mutation", prop_pos.clone());
                } else {
                    self.error(
                        RuleCode::S100,
                        format!("classes have no static member `{}`", prop),
                        prop_pos.clone(),
                    );
                }
                Some(self.err_expr(prop_pos))
            }
            Some(ScopeItem::Enum(id)) => {
                let member = self.enums[id.0]
                    .members
                    .iter()
                    .find(|(n, _)| n == prop)
                    .cloned();
                match member {
                    Some((member, value)) => Some(hir::Expr {
                        kind: ExprKind::EnumMember { id, member, value },
                        ty: Type::Enum(id),
                        pos: prop_pos,
                    }),
                    None => {
                        self.error(
                            RuleCode::S100,
                            format!("enum `{}` has no member `{}`", name, prop),
                            prop_pos.clone(),
                        );
                        Some(self.err_expr(prop_pos))
                    }
                }
            }
            Some(ScopeItem::StringAlias(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("string-literal union alias `{name}` has no static values"),
                    prop_pos.clone(),
                );
                Some(self.err_expr(prop_pos))
            }
            Some(ScopeItem::Global(_))
            | Some(ScopeItem::Func(_))
            | Some(ScopeItem::GenericFunc(_))
            | Some(ScopeItem::Foreign(_)) => None,
            None => {
                if name == "Object" {
                    if prop == "setPrototypeOf" {
                        self.error(RuleCode::S003, "no prototype mutation", prop_pos.clone());
                        return Some(self.err_expr(prop_pos));
                    }
                    if prop == "groupBy" {
                        self.reject_api_form(
                            "Object",
                            "groupBy",
                            "Object.groupBy",
                            prop_pos.clone(),
                        );
                        return Some(self.err_expr(prop_pos));
                    }
                }
                None
            }
        }
    }

    /// True when `obj` is the ambient `Math` namespace (stdlib.md §1):
    /// the identifier `Math` with no local binding and no program
    /// declaration shadowing it.
    fn is_math_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        let ast::Expr::Ident(id) = obj else {
            return false;
        };
        if id.sym.as_ref() != "Math" {
            return false;
        }
        let shadowed = fx.scopes.iter().rev().any(|s| s.vars.contains_key("Math"));
        !shadowed && self.scope_item("Math").is_none()
    }

    /// True when `obj` is the ambient `Context` namespace (Q6/Q7):
    /// the identifier `Context` with no local binding and no program
    /// declaration shadowing it.
    fn is_context_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        let ast::Expr::Ident(id) = obj else {
            return false;
        };
        if id.sym.as_ref() != "Context" {
            return false;
        }
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|s| s.vars.contains_key("Context"));
        !shadowed && self.scope_item("Context").is_none()
    }

    /// A `Math` member outside a call position (stdlib.md §1): a
    /// constant folds to its `f64` literal; anything else is rejected
    /// with the Q19 subset code.
    fn check_math_member(&mut self, prop: &str, prop_pos: Pos, for_write: bool) -> hir::Expr {
        if for_write {
            self.error(
                RuleCode::S014,
                format!("`Math.{}` is read-only (Q19)", prop),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        if let Some(value) = crate::ambient::math_const(prop) {
            return hir::Expr {
                kind: ExprKind::Float(value),
                ty: Type::F64,
                pos: prop_pos,
            };
        }
        if crate::ambient::math_fn(prop).is_some() {
            self.error(
                RuleCode::S014,
                format!("`Math.{}` may only be called, not read as a value", prop),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        self.error(
            RuleCode::S014,
            format!("`Math.{}` is outside the accepted Math subset (Q19)", prop),
            prop_pos.clone(),
        );
        self.err_expr(prop_pos)
    }

    /// A `Math.<fn>(…)` intrinsic call (stdlib.md §1): exact arity
    /// (Q19 — the lib's variadic `max`/`min`/`hypot` beyond two are out
    /// of subset). The binary32 bit-access members use their sized
    /// signatures from stdlib.md §17.1.
    fn check_math_call(
        &mut self,
        f: MathFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let arity = f.arity();
        if c.args.len() != arity {
            if matches!(f, MathFn::Hypot | MathFn::Max | MathFn::Min)
                && c.args.len() > 2
                && self.reject_api_form(
                    "Math",
                    "max/min/hypot with more than two arguments",
                    f.name(),
                    pos.clone(),
                )
            {
                return self.err_expr(pos);
            }
            let argument_type = match f {
                MathFn::Clz32 | MathFn::F32FromBits => "u32",
                MathFn::Imul => "i32",
                _ => "f64",
            };
            self.error(
                RuleCode::S014,
                format!(
                    "`Math.{}` takes exactly {} {} argument(s), got {} \
                     (Q19: the lib's variadic forms are out of subset)",
                    f.name(),
                    arity,
                    argument_type,
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let param_ty = match f {
            MathFn::Clz32 | MathFn::F32FromBits => Type::U32,
            MathFn::Imul => Type::I32,
            _ => Type::F64,
        };
        let params: Vec<ParamSig> = (0..arity)
            .map(|_| ParamSig {
                name: String::new(),
                ty: param_ty.clone(),
                has_default: false,
            })
            .collect();
        let args = self.check_args(&params, &c.args, fx, &pos, &format!("Math.{}", f.name()));
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Math(f),
                args,
            },
            ty: match f {
                MathFn::Clz32 | MathFn::Imul => Type::I32,
                MathFn::F32ToBits => Type::U32,
                _ => Type::F64,
            },
            pos,
        }
    }

    /// True when `obj` is the unshadowed ambient `Number` namespace.
    fn is_number_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        let ast::Expr::Ident(id) = obj else {
            return false;
        };
        if id.sym.as_ref() != "Number" {
            return false;
        }
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.vars.contains_key("Number"));
        !shadowed && self.scope_item("Number").is_none()
    }

    /// A `Number` namespace member outside a call position: constants
    /// fold to f64 literals; statics are call-only; all other members
    /// are rejected under Q25/Q27.
    fn check_number_member(&mut self, prop: &str, prop_pos: Pos, for_write: bool) -> hir::Expr {
        if for_write {
            self.error(
                RuleCode::S014,
                format!("`Number.{prop}` is read-only (Q25)"),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        if let Some(value) = crate::ambient::number_const(prop) {
            return hir::Expr {
                kind: ExprKind::Float(value),
                ty: Type::F64,
                pos: prop_pos,
            };
        }
        if crate::ambient::number_static(prop).is_some() {
            self.error(
                RuleCode::S014,
                format!("`Number.{prop}` may only be called, not read as a value (Q25)"),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        self.error(
            RuleCode::S014,
            format!("`Number.{prop}` is outside the accepted Number subset (Q25)"),
            prop_pos.clone(),
        );
        self.err_expr(prop_pos)
    }

    /// A `Number.is*` predicate call: exactly one `f64`, returning
    /// boolean through the shared Q25 runtime.
    fn check_number_predicate_call(
        &mut self,
        f: NumFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if c.args.len() != 1 {
            self.error(
                RuleCode::S014,
                format!(
                    "`Number.{}` takes exactly 1 f64 argument, got {} (Q25)",
                    f.name(),
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let args = self.check_args(
            &[ParamSig {
                name: String::new(),
                ty: Type::F64,
                has_default: false,
            }],
            &c.args,
            fx,
            &pos,
            &format!("Number.{}", f.name()),
        );
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Num(f),
                args,
            },
            ty: Type::Bool,
            pos,
        }
    }

    /// Global or `Number`-namespace `parseInt` / `parseFloat` calls.
    /// Both spellings lower to the same [`NumFn`] identity and runtime
    /// symbol. Their arity is part of Q25/Q27: in particular,
    /// parseInt's radix is required.
    fn check_number_global_call(
        &mut self,
        f: NumFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        call_name: &str,
    ) -> hir::Expr {
        let params: Vec<ParamSig> = match f {
            NumFn::ParseInt => vec![
                ParamSig {
                    name: String::new(),
                    ty: Type::Str,
                    has_default: false,
                },
                ParamSig {
                    name: String::new(),
                    ty: Type::I32,
                    has_default: false,
                },
            ],
            NumFn::ParseFloat => vec![ParamSig {
                name: String::new(),
                ty: Type::Str,
                has_default: false,
            }],
            _ => {
                self.error(
                    RuleCode::S100,
                    "internal Q25 parser identity mismatch",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        if c.args.len() != params.len() {
            if f == NumFn::ParseInt && c.args.len() == 1 && call_name == "parseInt" {
                self.reject_api_form("global", "parseInt(value)", "parseInt(value)", pos.clone());
            } else {
                self.error(
                    RuleCode::S014,
                    format!(
                        "`{}` takes exactly {} argument(s), got {} (Q25)",
                        call_name,
                        params.len(),
                        c.args.len()
                    ),
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        }
        let args = self.check_args(&params, &c.args, fx, &pos, call_name);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Num(f),
                args,
            },
            ty: Type::F64,
            pos,
        }
    }

    /// True when the name `Date` still resolves to the ambient
    /// namespace/constructor (stdlib.md §3): no function-local binding
    /// and no program declaration shadows it (same rule as `Math`).
    /// Consulted by both member access and `new Date(…)` so shadowing
    /// behaves identically in every position.
    fn date_is_ambient(&self, fx: &FnCtx) -> bool {
        let shadowed = fx.scopes.iter().rev().any(|s| s.vars.contains_key("Date"));
        !shadowed && self.scope_item("Date").is_none()
    }

    /// True when `Map` / `Set` resolves to the ambient generic reference
    /// class rather than a local or program declaration.
    fn assoc_is_ambient(&self, name: &str, fx: &FnCtx) -> bool {
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.vars.contains_key(name));
        !shadowed && self.scope_item(name).is_none()
    }

    /// True when `RegExp` resolves to the ambient constructor rather
    /// than a local or program declaration.
    fn regexp_is_ambient(&self, fx: &FnCtx) -> bool {
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.vars.contains_key("RegExp"));
        !shadowed && self.scope_item("RegExp").is_none()
    }

    /// True when `Worker` resolves to Q35's ambient static namespace rather
    /// than a local or program declaration.
    fn worker_is_ambient(&self, fx: &FnCtx) -> bool {
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.vars.contains_key("Worker"));
        !shadowed && self.scope_item("Worker").is_none()
    }

    /// Returns the innermost non-transferable field in one Q35 message
    /// payload. Value classes and FixedArray elements are descended
    /// recursively; a reference-class field is itself the offending leaf.
    fn non_transferable_message_field(
        &self,
        ty: &Type,
        path: &str,
        pos: &Pos,
        visiting: &mut std::collections::HashSet<ClassId>,
    ) -> Option<(Pos, String, Type)> {
        match ty {
            Type::I8
            | Type::U8
            | Type::I16
            | Type::U16
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::F16
            | Type::F32
            | Type::F64
            | Type::Bool
            | Type::Enum(_)
            | Type::StringAlias(_)
            | Type::Error => None,
            Type::FixedArray(element, _) => {
                self.non_transferable_message_field(element, path, pos, visiting)
            }
            Type::Class(id) if self.classes.get(id.0).is_some_and(|class| class.is_value) => {
                if !visiting.insert(*id) {
                    return None;
                }
                let class = &self.classes[id.0];
                let result = class.fields.iter().find_map(|field| {
                    let nested = format!("{path}.{}", field.name);
                    self.non_transferable_message_field(&field.ty, &nested, &field.pos, visiting)
                });
                visiting.remove(id);
                result
            }
            other => Some((pos.clone(), path.to_string(), other.clone())),
        }
    }

    fn validate_message_class(&mut self, id: ClassId) -> bool {
        let class = self.classes[id.0].clone();
        for field in &class.fields {
            let path = format!("{}.{}", class.name, field.name);
            if let Some((pos, path, ty)) = self.non_transferable_message_field(
                &field.ty,
                &path,
                &field.pos,
                &mut std::collections::HashSet::new(),
            ) {
                let type_name = self.type_name(&ty);
                self.error(
                    RuleCode::S100,
                    format!(
                        "message class `{}` is not transferable: innermost field `{path}` has non-transferable type `{type_name}`",
                        class.name
                    ),
                    pos,
                );
                return false;
            }
        }
        true
    }

    fn check_worker_spawn(&mut self, c: &ast::CallExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        if c.args.len() != 1
            || c.args
                .first()
                .is_some_and(|argument| argument.spread.is_some())
        {
            self.error(
                RuleCode::S100,
                format!(
                    "`Worker.spawn` expects one directly named entry function, got {} argument(s)",
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let argument = &c.args[0].expr;
        let mut entry_expr: &ast::Expr = argument;
        while let ast::Expr::Paren(paren) = entry_expr {
            entry_expr = &paren.expr;
        }
        let ast::Expr::Ident(ident) = entry_expr else {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must be a directly named module-level function; lambdas and other values are not worker entries",
                self.pos(argument.span()),
            );
            return self.err_expr(pos);
        };
        if fx
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.vars.contains_key(ident.sym.as_ref()))
        {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must name a module-level function directly, not a local function value",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        }
        let Some(ScopeItem::Func(function)) = self.scope_item(ident.sym.as_ref()) else {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must name a non-generic module-level function directly",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        };
        let Some(sig) = self.fn_sigs.get(&function).cloned() else {
            return self.err_expr(pos);
        };
        if sig.is_async {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must be synchronous; async worker entries are rejected",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        }
        if sig.is_generator
            || sig.ret != Type::Void
            || sig.params.len() != 2
            || sig.params.iter().any(|parameter| parameter.has_default)
        {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must have the exact synchronous shape `(inbox: Inbox<In>, outbox: Outbox<Out>) => void`",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        }
        let (input, output) = match (&sig.params[0].ty, &sig.params[1].ty) {
            (Type::Inbox(input), Type::Outbox(output)) => ((**input).clone(), (**output).clone()),
            _ => {
                self.error(
                    RuleCode::S100,
                    "`Worker.spawn` entry must have the exact synchronous shape `(inbox: Inbox<In>, outbox: Outbox<Out>) => void`",
                    self.pos(ident.span),
                );
                return self.err_expr(pos);
            }
        };
        let (Type::Class(input_id), Type::Class(output_id)) = (&input, &output) else {
            return self.err_expr(pos);
        };

        if let Some(type_args) = &c.type_args {
            if type_args.params.len() != 2 {
                self.error(
                    RuleCode::S100,
                    "`Worker.spawn` takes exactly two explicit type arguments",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let explicit_input = self.resolve_type(&type_args.params[0]);
            let explicit_output = self.resolve_type(&type_args.params[1]);
            if explicit_input != input || explicit_output != output {
                self.error(
                    RuleCode::S100,
                    "`Worker.spawn` type arguments must exactly match its entry's Inbox/Outbox message classes",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        }

        let mut valid = self.validate_message_class(*input_id);
        if input_id != output_id {
            valid &= self.validate_message_class(*output_id);
        }
        if !valid {
            return self.err_expr(pos);
        }
        let worker_entry = hir::WorkerEntry {
            function,
            input: *input_id,
            output: *output_id,
        };
        let entry_index = self
            .worker_entries
            .iter()
            .position(|entry| entry == &worker_entry)
            .unwrap_or_else(|| {
                let index = self.worker_entries.len();
                self.worker_entries.push(worker_entry);
                index
            });
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Worker(WorkerFn::Spawn(entry_index)),
                args: Vec::new(),
            },
            ty: Type::Worker(Box::new(input), Box::new(output)),
            pos,
        }
    }

    fn check_regex_new(&mut self, n: &ast::NewExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        if n.type_args.is_some() {
            self.error(RuleCode::S100, "`RegExp` is not generic", pos.clone());
        }
        let params = [
            ParamSig {
                name: "pattern".to_string(),
                ty: Type::Str,
                has_default: false,
            },
            ParamSig {
                name: "flags".to_string(),
                ty: Type::Str,
                has_default: true,
            },
        ];
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        let mut args = self.check_args(&params, args_ast, fx, &pos, "new RegExp");
        if args.len() == 1 {
            args.push(hir::Expr {
                kind: ExprKind::Str(String::new()),
                ty: Type::Str,
                pos: pos.clone(),
            });
        }
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Regex(RegexFn::New),
                args,
            },
            ty: Type::RegExp,
            pos,
        }
    }

    fn check_regex_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let (function, param, result) = match name {
            "test" => (RegexFn::Test, Type::Str, Type::Bool),
            "matchStart" => (RegexFn::MatchStart, Type::I32, Type::I32),
            "matchEnd" => (RegexFn::MatchEnd, Type::I32, Type::I32),
            "exec" => {
                self.error(
                    RuleCode::S014,
                    "`RegExp.exec` is rejected: its result needs an array with extra fields and a tuple type, neither of which the language has (Q31)",
                    prop_pos,
                );
                return self.err_expr(pos);
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    format!("`RegExp` has no accepted method `{name}`"),
                    prop_pos,
                );
                return self.err_expr(pos);
            }
        };
        let params = [ParamSig {
            name: String::new(),
            ty: param,
            has_default: false,
        }];
        let checked = self.check_args(&params, &c.args, fx, &pos, name);
        let mut args = Vec::with_capacity(2);
        args.push(recv);
        args.extend(checked);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Regex(function),
                args,
            },
            ty: result,
            pos,
        }
    }

    /// Checks the String methods whose first argument is overloaded
    /// between the standing literal-string pattern and P23's RegExp
    /// handle. The first argument is checked once so a regex literal
    /// cannot produce duplicate diagnostics.
    fn check_string_pattern_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let literal_without_global = name == "replaceAll"
            && c.args.first().is_some_and(|arg| {
                regex_literal(&arg.expr).is_some_and(|regex| !regex.flags.as_ref().contains('g'))
            });
        if literal_without_global {
            let diagnostic_pos = c
                .args
                .first()
                .map_or_else(|| pos.clone(), |arg| self.pos(arg.expr.span()));
            self.error(
                RuleCode::S100,
                "`string.replaceAll` with a RegExp literal requires the `g` flag",
                diagnostic_pos,
            );
        }
        let arity = if matches!(name, "replace" | "replaceAll") {
            2
        } else {
            1
        };
        if c.args.len() != arity || c.args.iter().any(|arg| arg.spread.is_some()) {
            self.error(
                RuleCode::S100,
                format!("`{name}` expects {arity} argument(s), got {}", c.args.len()),
                pos.clone(),
            );
        }
        let mut checked = Vec::with_capacity(c.args.len());
        for (index, arg) in c.args.iter().enumerate() {
            if arg.spread.is_some() {
                let spread_pos = self.pos(arg.spread.unwrap_or_default());
                self.error(
                    RuleCode::S014,
                    "spread arguments require variadic parameters, which the language does not have",
                    spread_pos,
                );
                continue;
            }
            let context = (index == 1).then_some(&Type::Str);
            let value = self.check_expr(&arg.expr, context, fx);
            if index == 1 {
                self.require_assignable(
                    &value.ty.clone(),
                    &Type::Str,
                    value.pos.clone(),
                    "the replacement",
                );
            }
            checked.push(value);
        }
        let Some(pattern) = checked.first() else {
            return self.err_expr(pos);
        };
        if literal_without_global {
            return self.err_expr(pos);
        }

        if pattern.ty == Type::RegExp {
            let function = match name {
                "search" => RegexFn::Search,
                "replace" => RegexFn::Replace,
                "replaceAll" => RegexFn::ReplaceAll,
                "split" => RegexFn::Split,
                _ => return self.err_expr(pos),
            };
            let mut args = Vec::with_capacity(1 + checked.len());
            args.push(recv);
            args.extend(checked);
            let ty = if name == "search" {
                Type::I32
            } else if name == "split" {
                Type::Array(Box::new(Type::Str))
            } else {
                Type::Str
            };
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Regex(function),
                    args,
                },
                ty,
                pos,
            };
        }

        if name == "search" {
            if pattern.ty != Type::Error {
                let message =
                    "`string.search` requires a `RegExp`; string-pattern search is not in the P23 surface (Q31)";
                self.error(RuleCode::S014, message, prop_pos);
            }
            return self.err_expr(pos);
        }

        self.require_assignable(
            &pattern.ty.clone(),
            &Type::Str,
            pattern.pos.clone(),
            "the pattern",
        );
        let function = match name {
            "replace" => StrFn::Replace,
            "replaceAll" => StrFn::ReplaceAll,
            "split" => StrFn::Split,
            _ => return self.err_expr(pos),
        };
        let mut args = Vec::with_capacity(1 + checked.len());
        args.push(recv);
        args.extend(checked);
        let ty = if name == "split" {
            Type::Array(Box::new(Type::Str))
        } else {
            Type::Str
        };
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Str(function),
                args,
            },
            ty,
            pos,
        }
    }

    /// True when `obj` is the ambient `Date` namespace (stdlib.md §3):
    /// the identifier `Date` with no local binding and no program
    /// declaration shadowing it (same rule as `Math`).
    fn is_date_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        let ast::Expr::Ident(id) = obj else {
            return false;
        };
        id.sym.as_ref() == "Date" && self.date_is_ambient(fx)
    }

    /// A `Date` namespace member outside a call position (stdlib.md §3):
    /// there are no constant members, so every read is a Q20 rejection.
    fn check_date_member(&mut self, prop: &str, prop_pos: Pos, for_write: bool) -> hir::Expr {
        if for_write {
            self.error(
                RuleCode::S014,
                format!("`Date.{}` is read-only (Q20)", prop),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        if prop == "parse"
            && self.reject_api_form("Date", "Date.parse", "Date.parse", prop_pos.clone())
        {
            return self.err_expr(prop_pos);
        }
        let why = if matches!(prop, "UTC" | "now") {
            format!(
                "`Date.{}` may only be called, not read as a value (Q20)",
                prop
            )
        } else {
            format!("`Date.{}` is outside the accepted Date subset (Q20)", prop)
        };
        self.error(RuleCode::S014, why, prop_pos.clone());
        self.err_expr(prop_pos)
    }

    /// A `Date` static call (`Date.UTC(…)`, `Date.now()`, stdlib.md §3).
    /// Returns `None` when `name` is not a static function member; the
    /// caller then falls through to the member rejection path.
    fn check_date_static_call(
        &mut self,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> Option<hir::Expr> {
        match name {
            "UTC" => {
                // year and month0 are required; day defaults to 1, the
                // time components to 0 (ECMA Date.UTC with the lib's
                // optional parameters).
                let params: Vec<ParamSig> = (0..7)
                    .map(|i| ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: i >= 2,
                    })
                    .collect();
                let mut args = self.check_args(&params, &c.args, fx, &pos, "Date.UTC");
                // Normalize to the fixed 7-argument runtime signature at
                // check time, so both tiers lower the identical call.
                while args.len() < 7 {
                    let default = if args.len() == 2 { 1 } else { 0 };
                    args.push(hir::Expr {
                        kind: ExprKind::Int(default),
                        ty: Type::I32,
                        pos: pos.clone(),
                    });
                }
                Some(hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Date(DateFn::Utc),
                        args,
                    },
                    ty: Type::I64,
                    pos,
                })
            }
            "now" => {
                let args = self.check_args(&[], &c.args, fx, &pos, "Date.now");
                Some(hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Date(DateFn::Now),
                        args,
                    },
                    ty: Type::I64,
                    pos,
                })
            }
            _ => None,
        }
    }

    /// `new Date(…)` when no program declaration shadows `Date`
    /// (stdlib.md §3): exactly one `i64` millisecond argument. The
    /// zero-argument and multi-argument lib constructors mean
    /// current/local time and are out of subset (Q20).
    fn check_date_new(&mut self, n: &ast::NewExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        match args_ast.len() {
            1 => {
                let params = [ParamSig {
                    name: String::new(),
                    ty: Type::I64,
                    has_default: false,
                }];
                let args = self.check_args(&params, args_ast, fx, &pos, "new Date");
                hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Date(DateFn::New),
                        args,
                    },
                    ty: Type::Date,
                    pos,
                }
            }
            0 => {
                self.reject_api_form("Date", "new Date()", "new Date()", pos.clone());
                self.err_expr(pos)
            }
            _ => {
                self.reject_api_form(
                    "Date",
                    "new Date(year, month, ...)",
                    "new Date(year, month, ...)",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    /// A method call on a `Date` receiver (stdlib.md §3): `getTime()`
    /// folds to the receiver value retyped `i64` (the identity on the
    /// representation — both tiers agree by construction), the UTC
    /// accessors and `toISOString` become intrinsics carrying the
    /// receiver as their first argument, and everything else is a Q20
    /// rejection.
    fn check_date_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        if name == "getTime" {
            self.check_args(&[], &c.args, fx, &pos, "getTime");
            return hir::Expr {
                kind: recv.kind,
                ty: Type::I64,
                pos,
            };
        }
        if let Some(f) = crate::ambient::date_method(name) {
            self.check_args(&[], &c.args, fx, &pos, name);
            let ty = if f == DateFn::ToIso {
                Type::Str
            } else {
                Type::I32
            };
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Date(f),
                    args: vec![recv],
                },
                ty,
                pos,
            };
        }
        self.date_subset_rejection(name, prop_pos.clone());
        self.err_expr(pos)
    }

    /// Emits the Q20 rejection for an out-of-subset `Date` instance
    /// member, naming the member and pointing at the accepted spelling.
    fn date_subset_rejection(&mut self, name: &str, pos: Pos) {
        let (code, why) = if let Some(rejection) = crate::ambient::date_rejection(name) {
            (
                rejection.code,
                crate::ambient::rejection_message(rejection, name),
            )
        } else {
            (
                RuleCode::S014,
                format!("`{}` is outside the accepted Date subset (Q20)", name),
            )
        };
        self.error(code, why, pos);
    }

    /// A Q25/Q26 numeric receiver method. The accepted formatting
    /// methods operate on `f32`/`f64`; methods with a shared `f64`
    /// runtime entry widen an `f32` receiver exactly in HIR.
    fn check_number_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        if !matches!(
            name,
            "toFixed" | "toString" | "toExponential" | "toPrecision"
        ) {
            if name == "toLocaleString" {
                self.reject_api_form("f32 / f64", "toLocaleString", "toLocaleString", prop_pos);
                return self.err_expr(pos);
            }
            let type_name = self.type_name(&recv.ty);
            self.error(
                RuleCode::S100,
                format!("`{type_name}` has no method `{name}`"),
                prop_pos,
            );
            return self.err_expr(pos);
        }
        if !matches!(&recv.ty, Type::F32 | Type::F64) {
            self.reject_api_form(
                "sized integers",
                "toFixed/toString/toExponential/toPrecision",
                name,
                prop_pos,
            );
            return self.err_expr(pos);
        }

        let (f, optional, arity_message) = match name {
            "toFixed" => (
                NumFn::ToFixed,
                false,
                "`toFixed` takes exactly 1 i32 digit count",
            ),
            "toString" => (
                if recv.ty == Type::F32 {
                    NumFn::ToStringF32
                } else {
                    NumFn::ToStringF64
                },
                false,
                "`toString` requires an explicit radix (2–36)",
            ),
            "toExponential" => (
                NumFn::ToExponential,
                true,
                "`toExponential` takes zero or one i32 digit count",
            ),
            "toPrecision" => (
                NumFn::ToPrecision,
                false,
                "`toPrecision` requires an explicit i32 digit count",
            ),
            _ => return self.err_expr(pos),
        };
        let arity_ok = if optional {
            c.args.len() <= 1
        } else {
            c.args.len() == 1
        };
        if !arity_ok {
            let documented = match (name, c.args.len()) {
                ("toString", 0) => Some("toString()"),
                ("toPrecision", 0) => Some("toPrecision()"),
                _ => None,
            };
            if documented.is_some_and(|surface| {
                self.reject_api_form("f32 / f64", surface, surface, pos.clone())
            }) {
                return self.err_expr(pos);
            }
            self.error(
                RuleCode::S014,
                format!("{arity_message}, got {} argument(s) (Q26)", c.args.len()),
                pos.clone(),
            );
            return self.err_expr(pos);
        }

        let params = [ParamSig {
            name: String::new(),
            ty: Type::I32,
            has_default: optional,
        }];
        let mut checked = self.check_args(&params, &c.args, fx, &pos, name);
        if optional && checked.is_empty() {
            checked.push(hir::Expr {
                kind: ExprKind::Int(-1),
                ty: Type::I32,
                pos: pos.clone(),
            });
        }

        let recv_pos = recv.pos.clone();
        let recv = if recv.ty == Type::F32 && f != NumFn::ToStringF32 {
            hir::Expr {
                kind: ExprKind::Cast(Box::new(recv)),
                ty: Type::F64,
                pos: recv_pos,
            }
        } else {
            recv
        };
        let mut args = Vec::with_capacity(2);
        args.push(recv);
        args.extend(checked);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Num(f),
                args,
            },
            ty: Type::Str,
            pos,
        }
    }

    /// A `String` method intrinsic call on a string receiver
    /// (stdlib.md §8, Q21/Q27). Optional arguments are normalized here:
    /// starting positions default to `0`, ending positions and lengths
    /// use `i32::MAX` as the runtime's "to the end" sentinel, and `pad`
    /// defaults to `" "`. Every runtime symbol therefore has a fixed
    /// arity and both tiers lower the identical call (the Date.UTC
    /// technique, §3). The receiver becomes the call's first argument.
    fn check_str_method(
        &mut self,
        recv: hir::Expr,
        f: StrFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let optional_slice = f == StrFn::Slice;
        let optional_zero_position =
            matches!(f, StrFn::IndexOf | StrFn::Includes | StrFn::StartsWith);
        let optional_end_position = matches!(f, StrFn::EndsWith | StrFn::Substring | StrFn::Substr);
        let optional_pad = matches!(f, StrFn::PadStart | StrFn::PadEnd);
        let params: Vec<ParamSig> = f
            .params()
            .iter()
            .enumerate()
            .map(|(i, p)| ParamSig {
                name: String::new(),
                ty: match p {
                    hir::StrParam::Str => Type::Str,
                    hir::StrParam::I32 => Type::I32,
                },
                has_default: optional_slice
                    || (i == 1
                        && (optional_zero_position || optional_end_position || optional_pad)),
            })
            .collect();
        let mut args = self.check_args(&params, &c.args, fx, &pos, f.name());
        if optional_slice && args.is_empty() {
            args.push(hir::Expr {
                kind: ExprKind::Int(0),
                ty: Type::I32,
                pos: pos.clone(),
            });
        }
        if args.len() + 1 == params.len() {
            if optional_slice || optional_end_position {
                args.push(hir::Expr {
                    kind: ExprKind::Int(i64::from(i32::MAX)),
                    ty: Type::I32,
                    pos: pos.clone(),
                });
            } else if optional_zero_position {
                args.push(hir::Expr {
                    kind: ExprKind::Int(0),
                    ty: Type::I32,
                    pos: pos.clone(),
                });
            } else if optional_pad {
                args.push(hir::Expr {
                    kind: ExprKind::Str(" ".to_string()),
                    ty: Type::Str,
                    pos: pos.clone(),
                });
            }
        }
        let ty = match f.ret() {
            hir::StrRet::I32 => Type::I32,
            hir::StrRet::Bool => Type::Bool,
            hir::StrRet::Str => Type::Str,
            hir::StrRet::StrArray => Type::Array(Box::new(Type::Str)),
        };
        let mut all = Vec::with_capacity(1 + args.len());
        all.push(recv);
        all.extend(args);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Str(f),
                args: all,
            },
            ty,
            pos,
        }
    }

    /// Emits the Q21 rejection for a known out-of-subset `String`
    /// member, naming the member and pointing at the accepted spelling;
    /// returns `false` when `name` is not in the rejected set (the
    /// caller then falls back to the generic surface diagnostic).
    fn str_subset_rejection(&mut self, name: &str, pos: Pos) -> bool {
        let Some(rejection) = crate::ambient::string_rejection(name) else {
            return false;
        };
        self.error(
            rejection.code,
            crate::ambient::rejection_message(rejection, name),
            pos,
        );
        true
    }

    /// The generic out-of-surface diagnostic for a string member that is
    /// neither accepted nor in the named Q21 rejected set.
    fn str_surface_error(&mut self, name: &str, pos: Pos) {
        self.error(
            RuleCode::S100,
            format!(
                "`{}` is outside the string surface (length, slice, `+` \
                 concatenation, `===`/`!==`, and the Q21 String methods)",
                name
            ),
            pos,
        );
    }

    /// The [`hir::ArrElemKind`] of an array element type under this
    /// program's class table (value classes excluded, stdlib.md §9).
    fn arr_elem_kind(&self, ty: &Type) -> Option<hir::ArrElemKind> {
        let classes = &self.classes;
        hir::ArrElemKind::of(ty, &|id| classes.get(id.0).is_some_and(|c| c.is_value))
    }

    /// Checks an accepted `Array` method call (stdlib.md §9, Q22):
    /// validates the arguments against the method's fixed shape,
    /// normalizes the optional ones (`join` separator, `slice`/`fill`
    /// range), types the callbacks under C5, and emits the
    /// [`Callee::Arr`] intrinsic with the receiver first.
    fn check_array_method(
        &mut self,
        recv: hir::Expr,
        elem: Type,
        f: ArrFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        use ArrFn as A;
        let arr_ty = Type::Array(Box::new(elem.clone()));
        let mk = |args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Arr(f),
                args,
            },
            ty,
            pos,
        };
        let int_default = |value: i64, pos: &Pos| hir::Expr {
            kind: ExprKind::Int(value),
            ty: Type::I32,
            pos: pos.clone(),
        };
        // The callback-taking methods (and the equality searches) move
        // element values across the runtime↔script boundary; the
        // checker gates the element kinds that can (Q22).
        let needs_elem_kind =
            f.takes_callback() || matches!(f, A::IndexOf | A::LastIndexOf | A::Includes);
        if needs_elem_kind && self.arr_elem_kind(&elem).is_none() {
            let elem_n = self.type_name(&elem);
            self.error(
                RuleCode::S014,
                format!(
                    "`{}` is defined per element kind (scalars, strings, `Date`, \
                     reference classes); `{}` elements are outside that set (Q22)",
                    f.name(),
                    elem_n
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        match f {
            A::IndexOf | A::LastIndexOf | A::Includes => {
                let params = [ParamSig {
                    name: String::new(),
                    ty: elem.clone(),
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, f.name()));
                let ty = if f == A::Includes {
                    Type::Bool
                } else {
                    Type::I32
                };
                mk(args, ty, pos)
            }
            A::Join => {
                if hir::ArrFmtKind::of(&elem).is_none() {
                    let elem_n = self.type_name(&elem);
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`join` formats elements by the Q14 interpolation rules; \
                             `{}` elements are not interpolatable (Q22)",
                            elem_n
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [ParamSig {
                    name: String::new(),
                    ty: Type::Str,
                    has_default: true,
                }];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "join");
                if checked.is_empty() {
                    checked.push(hir::Expr {
                        kind: ExprKind::Str(",".to_string()),
                        ty: Type::Str,
                        pos: pos.clone(),
                    });
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, Type::Str, pos)
            }
            A::Slice => {
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                ];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "slice");
                if checked.is_empty() {
                    checked.push(int_default(0, &pos));
                }
                if checked.len() == 1 {
                    checked.push(int_default(ArrFn::END_SENTINEL, &pos));
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, arr_ty, pos)
            }
            A::Fill => {
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: elem.clone(),
                        has_default: false,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                ];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "fill");
                // C5: `fill` stores its argument in the array.
                if let Some(value) = checked.first() {
                    if self.is_capturing_value(value, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not escape: `fill` stores its \
                             argument in the array",
                            value.pos.clone(),
                        );
                    }
                }
                if checked.len() == 1 {
                    checked.push(int_default(0, &pos));
                }
                if checked.len() == 2 {
                    checked.push(int_default(ArrFn::END_SENTINEL, &pos));
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, arr_ty, pos)
            }
            A::Reverse => {
                let args_checked = self.check_args(&[], &c.args, fx, &pos, "reverse");
                let mut args = vec![recv];
                args.extend(args_checked);
                mk(args, arr_ty, pos)
            }
            A::Concat => {
                let params = [ParamSig {
                    name: String::new(),
                    ty: arr_ty.clone(),
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "concat"));
                mk(args, arr_ty, pos)
            }
            A::Splice => {
                if c.args.len() > 2 {
                    self.reject_api_form(
                        "T[]",
                        "splice(start, deleteCount, ...items)",
                        "splice with inserted elements",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args.len() != 2 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`splice` expects 2 arguments (start, deleteCount), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: false,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: false,
                    },
                ];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "splice"));
                mk(args, arr_ty, pos)
            }
            A::Shift => {
                let mut args = vec![recv];
                args.extend(self.check_args(&[], &c.args, fx, &pos, "shift"));
                mk(args, elem, pos)
            }
            A::Unshift => {
                if c.args.len() > 1 {
                    self.reject_api_form(
                        "T[]",
                        "unshift(value, ...values)",
                        "unshift with multiple elements",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!("`unshift` expects 1 argument (value), got {}", c.args.len()),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [ParamSig {
                    name: String::new(),
                    ty: elem,
                    has_default: false,
                }];
                let checked = self.check_args(&params, &c.args, fx, &pos, "unshift");
                // C5: `unshift` stores its argument in the array.
                if let Some(value) = checked.first() {
                    if self.is_capturing_value(value, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not escape: `unshift` stores its \
                             argument in the array",
                            value.pos.clone(),
                        );
                    }
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, Type::I32, pos)
            }
            A::CopyWithin => {
                if !(2..=3).contains(&c.args.len()) {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`copyWithin` expects 2 or 3 arguments (target, start, end?), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: false,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: false,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                ];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "copyWithin");
                if checked.len() == 2 {
                    checked.push(int_default(ArrFn::END_SENTINEL, &pos));
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, arr_ty, pos)
            }
            A::Sort => {
                if c.args.is_empty() {
                    self.reject_api_form("T[]", "sort()", "sort()", pos.clone());
                    return self.err_expr(pos);
                }
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`sort` expects 1 argument (the comparator), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let cb = self.check_arr_callback(
                    &c.args[0],
                    vec![elem.clone(), elem],
                    Some(Type::I32),
                    fx,
                    "sort",
                    false,
                );
                mk(vec![recv, cb], arr_ty, pos)
            }
            A::Reduce | A::ReduceRight => {
                if c.args.len() < 2 {
                    let surface = if f == A::Reduce {
                        "reduce(callback)"
                    } else {
                        "reduceRight(callback)"
                    };
                    self.reject_api_form("T[]", surface, surface, pos.clone());
                    return self.err_expr(pos);
                }
                if c.args.len() != 2 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` expects 2 arguments (callback, init), got {}",
                            f.name(),
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if let Some(spread) = c.args[1].spread {
                    let p = self.pos(spread);
                    self.error(
                        RuleCode::S014,
                        "spread arguments require variadic parameters, which the language \
                         does not have",
                        p.clone(),
                    );
                    return self.err_expr(p);
                }
                // C4: the accumulator type `U` is the callback's when the
                // callback spells it (an annotated `acc` parameter, or a
                // function value's declared type) — that is `init`'s
                // natural contextual type, so a plain literal init does
                // not default to `i32` and poison `U`. Only an
                // un-annotated arrow leaves `U` to `init` itself.
                let (acc_ctx, checked_cb) = self.reduce_acc_context(&c.args[0], fx);
                let init = self.check_expr(&c.args[1].expr, acc_ctx.as_ref(), fx);
                if matches!(init.ty, Type::Error) {
                    return self.err_expr(pos);
                }
                let acc_ty = match &acc_ctx {
                    // The callback fixes `U`; a non-conforming init is
                    // reported against the init, not the callback.
                    Some(u) => {
                        self.require_assignable(
                            &init.ty,
                            u,
                            init.pos.clone(),
                            &format!("the `{}` init", f.name()),
                        );
                        u.clone()
                    }
                    None => init.ty.clone(),
                };
                if matches!(acc_ty, Type::Error) {
                    return self.err_expr(pos);
                }
                if self.arr_elem_kind(&acc_ty).is_none() {
                    let acc_n = self.type_name(&acc_ty);
                    self.error(
                        RuleCode::S014,
                        format!(
                            "the `{}` accumulator crosses the runtime↔script \
                             boundary; `{}` is outside the supported kinds (Q22)",
                            f.name(),
                            acc_n
                        ),
                        init.pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let cb = match checked_cb {
                    // Already checked (a function value): validate its
                    // shape, never check it twice.
                    Some(v) => self.expect_callback_shape(
                        v,
                        &[acc_ty.clone(), elem],
                        Some(&acc_ty),
                        f.name(),
                        true,
                    ),
                    None => self.check_arr_callback(
                        &c.args[0],
                        vec![acc_ty.clone(), elem],
                        Some(acc_ty.clone()),
                        fx,
                        f.name(),
                        true,
                    ),
                };
                mk(vec![recv, cb, init], acc_ty, pos)
            }
            A::ForEach | A::Map | A::Filter | A::Some | A::Every | A::FindIndex => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` expects 1 argument (the callback), got {}",
                            f.name(),
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let ret_ctx = match f {
                    A::ForEach => Some(Type::Void),
                    A::Map => None, // `U` inferred from the callback
                    _ => Some(Type::Bool),
                };
                let cb =
                    self.check_arr_callback(&c.args[0], vec![elem], ret_ctx, fx, f.name(), true);
                let ty = match f {
                    A::ForEach => Type::Void,
                    A::Filter => arr_ty,
                    A::Some | A::Every => Type::Bool,
                    A::FindIndex => Type::I32,
                    A::Map => {
                        let u = match &cb.ty {
                            Type::Func(ft) => ft.ret.clone(),
                            _ => Type::Error,
                        };
                        if matches!(u, Type::Error) {
                            return self.err_expr(pos);
                        }
                        if matches!(u, Type::Void) {
                            self.error(
                                RuleCode::S100,
                                "the `map` callback must return a value",
                                cb.pos.clone(),
                            );
                            return self.err_expr(pos);
                        }
                        if self.arr_elem_kind(&u).is_none() {
                            let u_n = self.type_name(&u);
                            self.error(
                                RuleCode::S014,
                                format!(
                                    "`map` produces a `{}[]`; `{}` is outside the \
                                     supported element kinds (Q22)",
                                    u_n, u_n
                                ),
                                cb.pos.clone(),
                            );
                            return self.err_expr(pos);
                        }
                        Type::Array(Box::new(u))
                    }
                    _ => Type::Error,
                };
                mk(vec![recv, cb], ty, pos)
            }
        }
    }

    /// True when `V` can carry `get`'s null miss: a reference class or
    /// opaque handle (including the built-in reference containers), or
    /// an already-nullable form of one.
    fn map_get_value_ok(&self, value: &Type) -> bool {
        self.is_reference_class(value)
            || matches!(value, Type::Array(_))
            || matches!(value, Type::Nullable(inner) if self.is_reference_class(inner))
    }

    /// Checks the Q27 static `Map.groupBy` intrinsic. Both generic
    /// arguments are inferred from the array and callback return, as for
    /// `Array.map`; the key result must be in the Q24 whitelist.
    fn check_map_group_by(&mut self, c: &ast::CallExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        if c.args.len() != 2 {
            self.error(
                RuleCode::S100,
                format!(
                    "`Map.groupBy` expects an array and a callback, got {} argument(s)",
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let items = self.check_expr(&c.args[0].expr, None, fx);
        let elem = match &items.ty {
            Type::Array(elem) => (**elem).clone(),
            Type::Error => return self.err_expr(pos),
            other => {
                let actual = self.type_name(other);
                self.error(
                    RuleCode::S100,
                    format!("`Map.groupBy` items must be a `T[]`, got `{actual}`"),
                    items.pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        let callback = self.check_arr_callback(
            &c.args[1],
            vec![elem.clone()],
            None,
            fx,
            "Map.groupBy",
            false,
        );
        let key = match &callback.ty {
            Type::Func(ft) if ft.ret != Type::Void => ft.ret.clone(),
            Type::Func(_) => {
                self.error(
                    RuleCode::S100,
                    "`Map.groupBy` callback must return a key",
                    callback.pos.clone(),
                );
                return self.err_expr(pos);
            }
            Type::Error => return self.err_expr(pos),
            other => {
                let actual = self.type_name(other);
                self.error(
                    RuleCode::S100,
                    format!("`Map.groupBy` callback is not a function, got `{actual}`"),
                    callback.pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        if self.assoc_key_kind(&key).is_none() {
            let key_name = self.type_name(&key);
            self.error(
                RuleCode::S014,
                format!(
                    "`Map.groupBy` callback returns `{key_name}`, which is not a \
                     §10.2 Map/Set key kind (Q24)"
                ),
                callback.pos.clone(),
            );
            return self.err_expr(pos);
        }
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Map(MapFn::GroupBy),
                args: vec![items, callback],
            },
            ty: Type::Map(Box::new(key), Box::new(Type::Array(Box::new(elem)))),
            pos,
        }
    }

    /// Checks a `Map<K, V>` intrinsic method (stdlib.md §10, Q24).
    fn check_map_method(
        &mut self,
        recv: hir::Expr,
        key: Type,
        value: Type,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let Some(operation) = crate::ambient::map_method(name) else {
            if let Some(rejection) = crate::ambient::map_rejection(name) {
                self.error(
                    rejection.code,
                    crate::ambient::rejection_message(rejection, name),
                    prop_pos,
                );
            } else {
                self.error(
                    RuleCode::S100,
                    format!("`Map` has no accepted method `{name}` (Q24)"),
                    prop_pos,
                );
            }
            return self.err_expr(pos);
        };
        let map_ty = Type::Map(Box::new(key.clone()), Box::new(value.clone()));
        let mk = |f: MapFn, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Map(f),
                args,
            },
            ty,
            pos,
        };
        match operation {
            MapFn::Get => {
                if !self.map_get_value_ok(&value) {
                    self.reject_api_form("Map<K, scalar V>", "get(key)", "get(key)", prop_pos);
                    return self.err_expr(pos);
                }
                let params = [ParamSig {
                    name: String::new(),
                    ty: key,
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "Map.get"));
                let ty = if matches!(value, Type::Nullable(_)) {
                    value
                } else {
                    Type::Nullable(Box::new(value))
                };
                mk(MapFn::Get, args, ty, pos)
            }
            MapFn::GetOr => {
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: key,
                        has_default: false,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: value.clone(),
                        has_default: false,
                    },
                ];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "Map.getOr"));
                mk(MapFn::GetOr, args, value, pos)
            }
            MapFn::Set => {
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: key,
                        has_default: false,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: value,
                        has_default: false,
                    },
                ];
                let checked = self.check_args(&params, &c.args, fx, &pos, "Map.set");
                if checked
                    .get(1)
                    .is_some_and(|argument| self.is_capturing_value(argument, fx))
                {
                    let value_pos = checked[1].pos.clone();
                    self.error(
                        RuleCode::S009,
                        "capturing lambdas may not escape: `Map.set` stores its value",
                        value_pos,
                    );
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(MapFn::Set, args, map_ty, pos)
            }
            MapFn::Has | MapFn::Delete => {
                let params = [ParamSig {
                    name: String::new(),
                    ty: key,
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(self.check_args(
                    &params,
                    &c.args,
                    fx,
                    &pos,
                    if operation == MapFn::Has {
                        "Map.has"
                    } else {
                        "Map.delete"
                    },
                ));
                mk(operation, args, Type::Bool, pos)
            }
            MapFn::Clear => {
                let checked = self.check_args(&[], &c.args, fx, &pos, "Map.clear");
                let mut args = vec![recv];
                args.extend(checked);
                mk(MapFn::Clear, args, Type::Void, pos)
            }
            MapFn::ForEach => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`Map.forEach` expects exactly 1 callback, got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let callback = self.check_arr_callback(
                    &c.args[0],
                    vec![value, key],
                    Some(Type::Void),
                    fx,
                    "Map.forEach",
                    false,
                );
                mk(MapFn::ForEach, vec![recv, callback], Type::Void, pos)
            }
            MapFn::New | MapFn::Size | MapFn::GroupBy => {
                self.error(
                    RuleCode::S100,
                    "internal Map member-table mismatch",
                    prop_pos,
                );
                self.err_expr(pos)
            }
        }
    }

    /// Checks a `Set<K>` intrinsic method (stdlib.md §10, Q24).
    fn check_set_method(
        &mut self,
        recv: hir::Expr,
        key: Type,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let Some(operation) = crate::ambient::set_method(name) else {
            if let Some(rejection) = crate::ambient::set_rejection(name) {
                self.error(
                    rejection.code,
                    crate::ambient::rejection_message(rejection, name),
                    prop_pos,
                );
            } else {
                self.error(
                    RuleCode::S100,
                    format!("`Set` has no accepted method `{name}` (Q24)"),
                    prop_pos,
                );
            }
            return self.err_expr(pos);
        };
        let set_ty = Type::Set(Box::new(key.clone()));
        let mk = |f: SetFn, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Set(f),
                args,
            },
            ty,
            pos,
        };
        match operation {
            SetFn::Add | SetFn::Has | SetFn::Delete => {
                let params = [ParamSig {
                    name: String::new(),
                    ty: key,
                    has_default: false,
                }];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, &format!("Set.{name}")));
                let ty = match operation {
                    SetFn::Add => set_ty,
                    SetFn::Has | SetFn::Delete => Type::Bool,
                    _ => Type::Error,
                };
                mk(operation, args, ty, pos)
            }
            SetFn::Clear => {
                let checked = self.check_args(&[], &c.args, fx, &pos, "Set.clear");
                let mut args = vec![recv];
                args.extend(checked);
                mk(SetFn::Clear, args, Type::Void, pos)
            }
            SetFn::ForEach => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`Set.forEach` expects exactly 1 callback, got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let callback = self.check_arr_callback(
                    &c.args[0],
                    vec![key],
                    Some(Type::Void),
                    fx,
                    "Set.forEach",
                    false,
                );
                mk(SetFn::ForEach, vec![recv, callback], Type::Void, pos)
            }
            SetFn::Union
            | SetFn::Intersection
            | SetFn::Difference
            | SetFn::SymmetricDifference
            | SetFn::IsSubsetOf
            | SetFn::IsSupersetOf
            | SetFn::IsDisjointFrom => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`Set.{name}` expects exactly 1 Set argument, got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args[0].spread.is_some() {
                    self.error(
                        RuleCode::S014,
                        "spread arguments require variadic parameters, which the language \
                         does not have",
                        self.pos(c.args[0].spread.unwrap_or_default()),
                    );
                    return self.err_expr(pos);
                }
                let other = self.check_expr(&c.args[0].expr, None, fx);
                match &other.ty {
                    Type::Set(other_key) => {
                        self.require_assignable(
                            other_key,
                            &key,
                            other.pos.clone(),
                            "the Set argument key",
                        );
                    }
                    Type::Error => return self.err_expr(pos),
                    _ => {
                        self.reject_api_form(
                            "Set<K>",
                            "algebra(non-Set)",
                            &format!("Set.{name} non-Set argument"),
                            other.pos.clone(),
                        );
                        return self.err_expr(pos);
                    }
                }
                let ty = if matches!(
                    operation,
                    SetFn::IsSubsetOf | SetFn::IsSupersetOf | SetFn::IsDisjointFrom
                ) {
                    Type::Bool
                } else {
                    set_ty
                };
                mk(operation, vec![recv, other], ty, pos)
            }
            SetFn::New | SetFn::Size => {
                self.error(
                    RuleCode::S100,
                    "internal Set member-table mismatch",
                    prop_pos,
                );
                self.err_expr(pos)
            }
        }
    }

    /// The accumulator type `U` a `reduce` callback spells, if any — the
    /// contextual type for `init` (C4).
    ///
    /// An arrow callback spells it as its first parameter's annotation:
    /// resolved here without reporting, because the callback check
    /// resolves the same annotation for real afterwards. Any other
    /// callback expression is a function value whose declared type
    /// already gives `U`; it is checked **once**, here, and returned so
    /// the caller shape-validates it rather than checking it again.
    /// `(None, None)` when the callback does not spell `U` (an
    /// un-annotated arrow, or an expression that is not a function),
    /// which leaves `init` context-free as before.
    fn reduce_acc_context(
        &mut self,
        arg: &ast::ExprOrSpread,
        fx: &mut FnCtx,
    ) -> (Option<Type>, Option<hir::Expr>) {
        if arg.spread.is_some() {
            return (None, None); // reported by `check_arr_callback`
        }
        if let ast::Expr::Arrow(a) = &*arg.expr {
            let Some(ast::Pat::Ident(binding)) = a.params.first() else {
                return (None, None);
            };
            let Some(ann) = binding.type_ann.as_ref() else {
                return (None, None);
            };
            let mark = self.diags.len();
            let ty = self.resolve_type(&ann.type_ann);
            self.diags.truncate(mark);
            return ((!matches!(ty, Type::Error)).then_some(ty), None);
        }
        let checked = self.check_expr(&arg.expr, None, fx);
        let acc = match &checked.ty {
            Type::Func(ft) => ft.params.first().cloned(),
            _ => None,
        };
        (acc, Some(checked))
    }

    /// Checks one callback argument of an `Array`, `Map`, or `Set`
    /// operation. Q27 Array callbacks accept the base parameter list and
    /// the same list with one trailing `i32` index; the fixed Q24
    /// callbacks and `sort` accept exactly the supplied list. `ret` is
    /// `None` when the return type is inferred from the callback (`map`
    /// and `Map.groupBy`).
    fn check_arr_callback(
        &mut self,
        arg: &ast::ExprOrSpread,
        params: Vec<Type>,
        ret: Option<Type>,
        fx: &mut FnCtx,
        method: &str,
        allow_index: bool,
    ) -> hir::Expr {
        if let Some(spread) = arg.spread {
            let p = self.pos(spread);
            self.error(
                RuleCode::S014,
                "spread arguments require variadic parameters, which the language does not have",
                p.clone(),
            );
            return self.err_expr(p);
        }
        let expr = &*arg.expr;
        if let ast::Expr::Arrow(a) = expr {
            let a_pos = self.pos(a.span);
            let Some(expected) = self.callback_params_for_arity(
                &params,
                a.params.len(),
                method,
                allow_index,
                a_pos.clone(),
            ) else {
                return self.err_expr(a_pos);
            };
            let checked = self.check_lambda_with(a, Some(&expected), ret.as_ref(), fx, a_pos);
            // An annotation may override the context; the resulting
            // function type must still match one accepted shape (the
            // return stays free when it is inferred).
            return self.expect_callback_shape(checked, &params, ret.as_ref(), method, allow_index);
        }
        // A function value (named reference or function-typed local).
        // A dual-arity Array callback has no single contextual function
        // type; named and local function values already carry their full
        // declared type and are validated below.
        let ctx_ty = (!allow_index).then(|| ret.as_ref()).flatten().map(|r| {
            Type::Func(Box::new(FuncType {
                params: params.clone(),
                ret: r.clone(),
            }))
        });
        let checked = self.check_expr(expr, ctx_ty.as_ref(), fx);
        self.expect_callback_shape(checked, &params, ret.as_ref(), method, allow_index)
    }

    /// Selects the contextual parameter list for a callback's source
    /// arity and emits the subset diagnostic when no accepted list has
    /// that length.
    fn callback_params_for_arity(
        &mut self,
        base: &[Type],
        actual: usize,
        method: &str,
        allow_index: bool,
        pos: Pos,
    ) -> Option<Vec<Type>> {
        if actual == base.len() {
            return Some(base.to_vec());
        }
        if allow_index && actual == base.len() + 1 {
            let mut indexed = base.to_vec();
            indexed.push(Type::I32);
            return Some(indexed);
        }
        if allow_index && actual == base.len() + 2 {
            let actual = format!("{method} callback with the container parameter");
            let _ = self.reject_api_form("T[]", "callback(value, index, array)", &actual, pos);
            return None;
        }
        let q_rule = if method == "Map.groupBy" {
            "Q27"
        } else if method.starts_with("Map.") || method.starts_with("Set.") {
            "Q24"
        } else {
            "Q22"
        };
        self.error(
            RuleCode::S014,
            if allow_index {
                format!(
                    "`{method}` callbacks take {} parameter(s), or {} with a trailing \
                     `i32` index; got {actual} (Q27)",
                    base.len(),
                    base.len() + 1,
                )
            } else if q_rule == "Q24" || q_rule == "Q27" {
                format!(
                    "`{method}` callbacks take exactly {} parameter(s); \
                     extra lib callback parameters are not accepted ({q_rule})",
                    base.len(),
                )
            } else {
                format!(
                    "`{method}` callbacks take exactly {} parameter(s); got {actual} (Q22)",
                    base.len()
                )
            },
            pos,
        );
        None
    }

    /// Validates a checked callback value against the method's accepted
    /// parameter list or lists (and return type, when it is not inferred);
    /// returns the value unchanged on success and a poisoned expression
    /// after the mismatch diagnostic otherwise.
    fn expect_callback_shape(
        &mut self,
        checked: hir::Expr,
        params: &[Type],
        ret: Option<&Type>,
        method: &str,
        allow_index: bool,
    ) -> hir::Expr {
        let ok = match &checked.ty {
            Type::Error => true,
            Type::Func(ft) => {
                let indexed = allow_index
                    && ft.params.len() == params.len() + 1
                    && ft.params[..params.len()] == *params
                    && ft.params.last() == Some(&Type::I32);
                (ft.params == params || indexed) && ret.is_none_or(|r| ft.ret == *r)
            }
            _ => false,
        };
        if ok {
            return checked;
        }
        if let Type::Func(ft) = &checked.ty {
            let accepted_arity = ft.params.len() == params.len()
                || (allow_index && ft.params.len() == params.len() + 1);
            if !accepted_arity {
                let pos = checked.pos.clone();
                let _ = self.callback_params_for_arity(
                    params,
                    ft.params.len(),
                    method,
                    allow_index,
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        }
        let got = self.type_name(&checked.ty);
        let wanted: Vec<String> = params.iter().map(|t| self.type_name(t)).collect();
        let wanted = if allow_index {
            format!("({}) or ({}, i32)", wanted.join(", "), wanted.join(", "))
        } else {
            format!("({})", wanted.join(", "))
        };
        let ret_n = match ret {
            Some(r) => self.type_name(r),
            None => "…".to_string(),
        };
        self.error(
            RuleCode::S100,
            format!(
                "type mismatch: the `{}` callback expects `{}` => {}, got `{}`",
                method, wanted, ret_n, got
            ),
            checked.pos.clone(),
        );
        self.err_expr(checked.pos)
    }

    /// Emits the Q22 rejection for a known out-of-subset `Array`
    /// member, naming the member and pointing at the accepted spelling;
    /// returns `false` when `name` is not in the rejected set (the
    /// caller then falls back to the generic surface diagnostic).
    fn arr_subset_rejection(&mut self, name: &str, pos: Pos) -> bool {
        let Some(rejection) = crate::ambient::array_rejection(name) else {
            return false;
        };
        self.error(
            rejection.code,
            crate::ambient::rejection_message(rejection, name),
            pos,
        );
        true
    }

    /// The generic out-of-surface diagnostic for an array member that is
    /// neither accepted nor in the named Q22 rejected set.
    fn arr_surface_error(&mut self, name: &str, pos: Pos) {
        self.error(
            RuleCode::S100,
            format!(
                "`{}` is outside the array surface (length, indexing, push, pop, \
                 and the Q22 Array methods)",
                name
            ),
            pos,
        );
    }

    fn check_member_read(&mut self, m: &ast::MemberExpr, fx: &mut FnCtx) -> hir::Expr {
        self.check_member_read_inner(m, fx, false)
    }

    fn check_member_read_inner(
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
                    self.error(
                        RuleCode::S100,
                        "an absence-capable descriptor member may be read only after an `!== undefined` presence test",
                        expr.pos.clone(),
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

    fn is_absence_capable_member_expr(&self, expr: &hir::Expr) -> bool {
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

    fn check_index(&mut self, obj: hir::Expr, index: hir::Expr, pos: Pos) -> hir::Expr {
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
    fn member_on(
        &mut self,
        obj: hir::Expr,
        name: &str,
        prop_pos: Pos,
        for_write: bool,
    ) -> hir::Expr {
        if name == "prototype" {
            self.error(RuleCode::S003, "no prototype mutation", prop_pos.clone());
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
                let class_name = self.classes[id.0].name.clone();
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
                } else {
                    self.error(
                        RuleCode::S100,
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
                    self.error(
                        rejection.code,
                        crate::ambient::rejection_message(rejection, name),
                        prop_pos.clone(),
                    );
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
                    self.error(
                        rejection.code,
                        crate::ambient::rejection_message(rejection, name),
                        prop_pos.clone(),
                    );
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
                self.error(
                    if matches!(name, "lastIndex" | "exec") {
                        RuleCode::S014
                    } else {
                        RuleCode::S100
                    },
                    message,
                    prop_pos.clone(),
                );
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
                        RuleCode::S100,
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
                    RuleCode::S100,
                    format!("`{}` has no member `{}`", type_name, name),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
        }
    }

    // ----- assignment -----

    fn check_assign(
        &mut self,
        a: &ast::AssignExpr,
        fx: &mut FnCtx,
        pos: Pos,
        statement_position: bool,
    ) -> hir::Expr {
        use ast::AssignOp as A;
        let op = match a.op {
            A::Assign => None,
            A::AddAssign => Some(BinOp::Add),
            A::SubAssign => Some(BinOp::Sub),
            A::MulAssign => Some(BinOp::Mul),
            A::DivAssign => Some(BinOp::Div),
            A::ModAssign => Some(BinOp::Rem),
            A::BitAndAssign => Some(BinOp::BitAnd),
            A::BitOrAssign => Some(BinOp::BitOr),
            A::BitXorAssign => Some(BinOp::BitXor),
            A::LShiftAssign => Some(BinOp::Shl),
            A::RShiftAssign => Some(BinOp::Shr),
            A::ZeroFillRShiftAssign => Some(BinOp::UShr),
            _ => {
                self.error(
                    RuleCode::S100,
                    "assignment operator outside the decided surface",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        let target = self.check_assign_target(&a.left, fx, &pos);
        let target_ty = target.ty.clone();
        let value_ctx = if matches!(target_ty, Type::Error) {
            None
        } else {
            Some(target_ty.clone())
        };
        let value = self.check_expr(&a.right, value_ctx.as_ref(), fx);
        let signature_write = match &target.kind {
            ExprKind::Call {
                callee: Callee::Method { recv, name },
                args,
            } if name == "get" && args.len() == 1 => match &recv.ty {
                Type::Class(id) => self.classes[id.0]
                    .index_signature
                    .clone()
                    .map(|signature| ((**recv).clone(), args[0].clone(), signature)),
                _ => None,
            },
            _ => None,
        };
        if let Some((recv, index, signature)) = signature_write {
            if op.is_some() {
                let operator = match a.op {
                    A::AddAssign => "+=",
                    A::SubAssign => "-=",
                    A::MulAssign => "*=",
                    A::DivAssign => "/=",
                    A::ModAssign => "%=",
                    A::BitAndAssign => "&=",
                    A::BitOrAssign => "|=",
                    A::BitXorAssign => "^=",
                    A::LShiftAssign => "<<=",
                    A::RShiftAssign => ">>=",
                    A::ZeroFillRShiftAssign => ">>>=",
                    _ => "op=",
                };
                self.error(
                    RuleCode::S100,
                    format!("`a[i] {operator} v` is not supported for a class index signature"),
                    pos.clone(),
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
            if !statement_position {
                self.error(
                    RuleCode::S100,
                    "`a[i] = v` cannot be used as a value",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            self.require_assignable(
                &value.ty.clone(),
                &signature.element_ty,
                value.pos.clone(),
                "the assignment",
            );
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(recv),
                        name: "set".to_string(),
                    },
                    args: vec![index, value],
                },
                ty: Type::Void,
                pos,
            };
        }
        if let Some(bin) = op {
            // Compound assignment is same-type arithmetic on the target.
            if target_ty == Type::F16
                && matches!(
                    bin,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
                )
            {
                self.error(
                    RuleCode::S014,
                    "arithmetic on `f16` is not supported; compute via `as f32`",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let numeric_ok = match bin {
                BinOp::Add => target_ty.is_numeric() || target_ty == Type::Str,
                BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => target_ty.is_numeric(),
                _ => target_ty.is_integer(),
            };
            if !numeric_ok && !matches!(target_ty, Type::Error) {
                let name = self.type_name(&target_ty);
                self.error(
                    RuleCode::S100,
                    format!("compound assignment is not defined for `{}`", name),
                    pos.clone(),
                );
            }
            let literal_shift_amount = match &value.kind {
                ExprKind::Int(bits) => int_value_at_type(*bits, &value.ty),
                _ => None,
            };
            if matches!(bin, BinOp::Shl | BinOp::Shr | BinOp::UShr)
                && literal_shift_amount.is_some_and(|amount| {
                    amount >= i128::from(integer_width(&target_ty).unwrap_or(i64::MAX))
                })
            {
                let amount = literal_shift_amount.unwrap_or(0);
                let width = integer_width(&target_ty).unwrap_or(0);
                let name = self.type_name(&target_ty);
                self.error(
                    RuleCode::S008,
                    format!(
                        "literal shift amount {} is out of range for `{}` width {}",
                        amount, name, width
                    ),
                    value.pos.clone(),
                );
            }
        }
        self.require_assignable(
            &value.ty.clone(),
            &target_ty,
            value.pos.clone(),
            "the assignment",
        );
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
            ty: target_ty,
            pos,
        }
    }

    fn check_assign_target(
        &mut self,
        target: &ast::AssignTarget,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> hir::Expr {
        match target {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(binding)) => {
                let name = binding.id.sym.to_string();
                let ident_pos = self.pos(binding.id.span);
                if let Some(local) = self.lookup_local(&name, &ident_pos, fx) {
                    if !local.mutable {
                        self.error(
                            RuleCode::S100,
                            format!("cannot rebind `const` binding `{}`", name),
                            ident_pos.clone(),
                        );
                    }
                    return hir::Expr {
                        kind: ExprKind::Local(name),
                        ty: local.ty,
                        pos: ident_pos,
                    };
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
                        return hir::Expr {
                            kind: ExprKind::Global(g),
                            ty: sig.ty,
                            pos: ident_pos,
                        };
                    }
                }
                self.error(
                    RuleCode::S100,
                    format!("`{}` is not an assignable binding", name),
                    ident_pos.clone(),
                );
                self.err_expr(ident_pos)
            }
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(m)) => {
                self.check_member_write(m, fx)
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    "assignment target outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos.clone())
            }
        }
    }

    fn check_member_write(&mut self, m: &ast::MemberExpr, fx: &mut FnCtx) -> hir::Expr {
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
                    self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx, true)
                {
                    return handled;
                }
                let obj = self.check_receiver(&m.obj, fx);
                self.member_on(obj, &name, prop_pos, true)
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

    // ----- calls -----

    fn check_call(
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

    fn check_named_call(
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
        match self.scope_item(&name) {
            Some(ScopeItem::Func(f)) => {
                if c.type_args.is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("`{}` is not generic", name),
                        ident_pos.clone(),
                    );
                }
                self.check_direct_call(&f, c, fx, pos)
            }
            Some(ScopeItem::Foreign(f)) => {
                if c.type_args.is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("`{}` is not generic", name),
                        ident_pos.clone(),
                    );
                }
                self.check_foreign_call(&f, c, fx, pos)
            }
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
                    self.error(RuleCode::S002, "no dynamic code evaluation", pos.clone());
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
                        self.error(
                            RuleCode::S100,
                            "`unreachable()` is only legal as a call statement",
                            pos.clone(),
                        );
                        let _ = self.check_ambient_call(ambient, c, fx, pos.clone());
                        return self.err_expr(pos);
                    }
                    return self.check_ambient_call(ambient, c, fx, pos);
                }
                self.error(
                    RuleCode::S100,
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
            self.error(
                RuleCode::S013,
                format!("async call `{fn_name}(...)` must be immediately awaited"),
                pos.clone(),
            );
            return self.err_expr(pos);
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
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Func(fn_name.to_string()),
                args,
            },
            ty: sig.ret,
            pos,
        }
    }

    /// Checks a call to a foreign C-ABI function declared by an ambient
    /// mirror (P5.2). Type-checks arguments against the mapped boundary
    /// signature and emits a [`Callee::Foreign`] call; no lowering path
    /// exists yet (P5.2b).
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
            .map(|t| ParamSig {
                name: String::new(),
                ty: t.clone(),
                has_default: false,
            })
            .collect();
        let name = match ambient {
            AmbientFn::Print => "print",
            AmbientFn::Unreachable => "unreachable",
            AmbientFn::Collect => "Context.collect",
            AmbientFn::UnsafeDelete => "Context.free",
        };
        let args = self.check_args(&params, &c.args, fx, &pos, name);
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
            self.error(
                RuleCode::S100,
                format!(
                    "`Context.{name}<T>` cannot use `{target_name}`; it is not a @CStruct value class or FixedArray"
                ),
                member_pos,
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
            self.error(
                RuleCode::S100,
                format!("`Context.{name}<T>` cannot use `{target_name}`; {detail}"),
                member_pos,
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

    fn check_indirect_call(
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
                    .map(|t| ParamSig {
                        name: String::new(),
                        ty: t.clone(),
                        has_default: false,
                    })
                    .collect();
                let args = self.check_args(&params, &c.args, fx, &pos, "the function value");
                hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Value(Box::new(callee)),
                        args,
                    },
                    ty: ft.ret.clone(),
                    pos,
                }
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
            self.error(
                RuleCode::S013,
                format!("Promise combinator `.{name}(...)` is not in the language"),
                prop_pos.clone(),
            );
            return self.err_expr(pos);
        }
        if matches!(&*m.obj, ast::Expr::Ident(id) if id.sym.as_ref() == "Promise")
            && !fx
                .scopes
                .iter()
                .rev()
                .any(|scope| scope.vars.contains_key("Promise"))
            && self.scope_item("Promise").is_none()
        {
            self.error(
                RuleCode::S013,
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
            self.error(
                RuleCode::S100,
                format!("`Worker` has no static method `{name}`"),
                prop_pos,
            );
            return self.err_expr(pos);
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
                self.check_map_method(recv, *key, *value, &name, c, fx, pos, prop_pos)
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
                            RuleCode::S100,
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
                            RuleCode::S100,
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
                        RuleCode::S100,
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
                    let params = [ParamSig {
                        name: String::new(),
                        ty: (*elem).clone(),
                        has_default: false,
                    }];
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
                    RuleCode::S100,
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
                    match super::layout::class_independent_layout(&step) {
                        super::layout::IndependentLayout::Fits => mk(recv, args, step, pos),
                        super::layout::IndependentLayout::TooLarge => {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "coroutine step-result layout exceeds the supported \
                                     aggregate limit of {} bytes",
                                    crate::types::MAX_AGGREGATE_BYTES
                                ),
                                prop_pos,
                            );
                            self.err_expr(pos)
                        }
                        super::layout::IndependentLayout::DependsOnClass => {
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
                            let class_name = self.classes[id.0].name.clone();
                            self.error(
                                RuleCode::S013,
                                format!(
                                    "async method call `{class_name}.{name}(...)` must be immediately awaited"
                                ),
                                pos.clone(),
                            );
                            return self.err_expr(pos);
                        }
                        let args = self.check_args(&sig.params, &c.args, fx, &pos, &name);
                        mk(recv, args, sig.ret, pos)
                    }
                    None => {
                        let class_name = self.classes[id.0].name.clone();
                        self.error(
                            RuleCode::S100,
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
                    RuleCode::S100,
                    format!("`{}` has no method `{}`", type_name, name),
                    prop_pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    fn check_args(
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
            }
            out.push(checked);
        }
        out
    }

    fn check_new(&mut self, n: &ast::NewExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
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
        if name == "Function" {
            self.error(
                RuleCode::S002,
                "no dynamic code evaluation (`new Function`)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "Promise" {
            self.error(
                RuleCode::S013,
                "Promise objects cannot be constructed; async functions expose no Promise object surface",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if matches!(name.as_str(), "Worker" | "Inbox" | "Outbox")
            && self.scope_item(&name).is_none()
        {
            self.error(
                RuleCode::S100,
                format!("`new {name}` is rejected; Q35 worker handles and endpoints are runtime-created"),
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
            if n.args.as_ref().is_some_and(|args| !args.is_empty()) {
                self.reject_api_form(
                    "Map / Set",
                    "new Map/Set(iterable)",
                    &format!("new {name}(iterable)"),
                    pos.clone(),
                );
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
            let ty = Type::Set(Box::new(key));
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Set(SetFn::New),
                    args: Vec::new(),
                },
                ty,
                pos,
            };
        }
        let class_id = match self.scope_item(&name) {
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
                self.error(
                    RuleCode::S100,
                    format!("unknown class `{}`", name),
                    ident_pos.clone(),
                );
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
        if self.classes[class_id.0].is_descriptor {
            self.error(
                RuleCode::S100,
                format!(
                    "descriptor class `{name}` is constructed with an object literal, not `new`"
                ),
                pos.clone(),
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

    // ----- lambdas (C5) -----

    fn check_lambda(
        &mut self,
        a: &ast::ArrowExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let ctx_fn = match ctx {
            Some(Type::Func(ft)) => Some((**ft).clone()),
            _ => None,
        };
        let (param_ctx, ret_ctx) = match &ctx_fn {
            Some(ft) => (Some(ft.params.as_slice()), Some(&ft.ret)),
            None => (None, None),
        };
        self.check_lambda_with(a, param_ctx, ret_ctx, fx, pos)
    }

    /// [`Self::check_lambda`] with the contextual function type split
    /// into its two halves, so a caller can supply parameter context
    /// while leaving the return type to be inferred from the body (the
    /// `map` callback, stdlib.md §9: `U` comes from the closure).
    fn check_lambda_with(
        &mut self,
        a: &ast::ArrowExpr,
        param_ctx: Option<&[Type]>,
        ret_ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if a.is_async {
            self.error(
                RuleCode::S100,
                "async arrow functions are not in the decided surface; use an async function declaration",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if a.is_generator {
            self.error(
                RuleCode::S100,
                "generator arrows are not in the decided surface",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let mut params = Vec::new();
        for (i, pat) in a.params.iter().enumerate() {
            // An un-annotated lambda parameter takes its type from the
            // contextual function type (tsc-style contextual typing);
            // only a parameter with neither annotation nor context is an
            // error. This is how a boundary callback (e.g. a `void*`
            // `object | null` userdata slot) is typed without the program
            // spelling the boundary type itself.
            let unannotated_ident = match pat {
                ast::Pat::Ident(b) if b.type_ann.is_none() && !b.id.optional => Some(b),
                _ => None,
            };
            let sig = if let Some(b) = unannotated_ident {
                if let Some(t) = param_ctx.and_then(|p| p.get(i)) {
                    ParamSig {
                        name: b.id.sym.to_string(),
                        ty: t.clone(),
                        has_default: false,
                    }
                } else {
                    self.resolve_param_pat(pat)
                }
            } else {
                self.resolve_param_pat(pat)
            };
            params.push(sig);
        }
        let mut ret = a
            .return_type
            .as_ref()
            .map(|ann| self.resolve_type(&ann.type_ann))
            .or_else(|| ret_ctx.cloned());

        fx.frames.push(Frame {
            ret: ret.clone().unwrap_or(Type::Error),
            is_generator: false,
            is_async: false,
            yield_ty: None,
            is_lambda: true,
            captures: Vec::new(),
            this_ty: None,
        });
        fx.scopes.push(Scope {
            vars: std::collections::HashMap::new(),
            fn_boundary: true,
        });
        // Lambda bodies start without the enclosing narrowing facts
        // (conservative: the lambda may run later).
        let saved_narrowed = std::mem::take(&mut fx.narrowed);
        let mut hir_params = Vec::new();
        for p in &params {
            fx.declare(
                &p.name,
                Local {
                    ty: p.ty.clone(),
                    mutable: true,
                    holds_capturing: false,
                },
            );
            hir_params.push(hir::Param {
                name: p.name.clone(),
                ty: p.ty.clone(),
                default: None,
                foreign_provenance: None,
                pos: pos.clone(),
            });
        }
        let body = match &*a.body {
            ast::BlockStmtOrExpr::Expr(e) => {
                let checked = self.check_expr(e, ret.as_ref(), fx);
                if let Some(ret) = &ret {
                    self.require_assignable(
                        &checked.ty.clone(),
                        &ret.clone(),
                        checked.pos.clone(),
                        "the lambda body",
                    );
                } else {
                    ret = Some(checked.ty.clone());
                }
                let value_pos = checked.pos.clone();
                vec![hir::Stmt::Return {
                    value: Some(checked),
                    pos: value_pos,
                }]
            }
            ast::BlockStmtOrExpr::BlockStmt(block) => {
                if ret.is_none() {
                    self.error(
                        RuleCode::S100,
                        "a lambda with a block body requires a return type annotation",
                        pos.clone(),
                    );
                    ret = Some(Type::Error);
                }
                if let Some(frame) = fx.frames.last_mut() {
                    frame.ret = ret.clone().unwrap_or(Type::Error);
                }
                let mut out = Vec::new();
                for s in &block.stmts {
                    self.check_stmt(s, fx, &mut out);
                }
                if let Some(ret) = &ret {
                    if !matches!(ret, Type::Void | Type::Error)
                        && !super::stmt::always_returns(&out)
                    {
                        self.error(RuleCode::S100, "not all paths return a value", pos.clone());
                    }
                }
                out
            }
        };
        fx.narrowed = saved_narrowed;
        fx.scopes.pop();
        let frame = fx.frames.pop();
        let captures = frame.map(|f| f.captures).unwrap_or_default();
        let ret = ret.unwrap_or(Type::Error);
        let ty = Type::Func(Box::new(FuncType {
            params: params.iter().map(|p| p.ty.clone()).collect(),
            ret: ret.clone(),
        }));
        hir::Expr {
            kind: ExprKind::Lambda {
                params: hir_params,
                ret,
                body,
                captures,
            },
            ty,
            pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_program, RuleCode, SourceFile};

    #[test]
    fn f32_from_bits_rejects_an_f64_argument_with_s007() {
        let source = "export function main(): void {\n  const value: f64 = 1.0;\n  print(`${Math.f32FromBits(value)}`);\n}\n";
        let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
            .expect_err("f32FromBits rejects an f64 argument");
        assert_eq!(diagnostics[0].code, RuleCode::S007);
        assert_eq!(diagnostics[0].pos.line, 3);
    }
}
