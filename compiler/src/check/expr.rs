//! Expression checking: contextual literal typing (C4), sized-numeric
//! arithmetic (C3/Q18), nominal member access (C1/Q4/Q5), calls, `as`
//! conversions, lambdas (C5), and null narrowing at use sites (C7).

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Pos, RuleCode};
use crate::hir::{
    self, AmbientFn, ArrFn, BinOp, Callee, DateFn, ExprKind, MapFn, MathFn, NumFn, SetFn,
    StrFn, TplPart, UnOp,
};
use crate::types::{FuncType, Type};

use super::{Checker, FnCtx, Frame, Local, ParamSig, Scope, ScopeItem};

/// Dotted path key for narrowing (`node`, `node.next`, `this.x`).
pub(crate) fn path_key(e: &hir::Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Local(n) | ExprKind::Global(n) => Some(n.clone()),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Field { obj, name } => path_key(obj).map(|p| format!("{}.{}", p, name)),
        _ => None,
    }
}

/// True for expressions that are numeric literals possibly wrapped in
/// parentheses or unary minus; these adopt the sized type of their
/// context (C4).
fn literalish(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Lit(ast::Lit::Num(_)) => true,
        ast::Expr::Paren(p) => literalish(&p.expr),
        ast::Expr::Unary(u) if u.op == ast::UnaryOp::Minus => literalish(&u.arg),
        _ => false,
    }
}

fn int_range(ty: &Type) -> Option<(i64, i64)> {
    // i64/u64 literals are capped at the f64-exact range; larger
    // spellings are out of the surface syntax (C3).
    const EXACT: i64 = 9_007_199_254_740_991; // 2^53 - 1
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
            ast::Expr::Assign(a) => self.check_assign(a, fx, pos),
            ast::Expr::Member(m) => self.check_member_read(m, fx),
            ast::Expr::Cond(c) => self.check_cond(c, ctx, fx, pos),
            ast::Expr::Call(c) => self.check_call(c, fx, pos),
            ast::Expr::New(n) => self.check_new(n, fx, pos),
            ast::Expr::Arrow(a) => self.check_lambda(a, ctx, fx, pos),
            ast::Expr::Array(a) => self.check_array_lit(a, ctx, fx, pos),
            ast::Expr::Object(_) => {
                if matches!(ctx, Some(Type::Class(_))) {
                    self.error(
                        RuleCode::S005,
                        "object literals do not satisfy nominal class types",
                        pos.clone(),
                    );
                } else {
                    self.error(
                        RuleCode::S100,
                        "object literals are not in the decided surface",
                        pos.clone(),
                    );
                }
                self.err_expr(pos)
            }
            ast::Expr::TsAs(a) => self.check_as(a, fx, pos),
            ast::Expr::Yield(y) => self.check_yield(y, fx, pos),
            ast::Expr::Await(_) => {
                self.error(
                    RuleCode::S013,
                    "`await` requires an event loop; the language has none",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
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

    fn check_lit(&mut self, lit: &ast::Lit, ctx: Option<&Type>, pos: Pos) -> hir::Expr {
        match lit {
            ast::Lit::Num(n) => self.check_num_lit(n, false, ctx, pos),
            ast::Lit::Str(s) => hir::Expr {
                kind: ExprKind::Str(s.value.to_string()),
                ty: Type::Str,
                pos,
            },
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
        let fractional =
            raw.contains('.') || (!hex && (raw.contains('e') || raw.contains('E')));
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
        let (lo, hi) = int_range(&target).unwrap_or((i64::MIN, i64::MAX));
        if value < lo as f64 || value > hi as f64 {
            let name = self.type_name(&target);
            self.error(
                RuleCode::S008,
                format!("integer literal {} out of range for `{}`", raw, name),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        hir::Expr {
            kind: ExprKind::Int(value as i64),
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
                        Type::Str | Type::Bool | Type::Enum(_) | Type::Error
                    );
                if checked.ty == Type::Date {
                    // Q20: a Date has no implicit string form (the lib's
                    // would be local-time `toString`).
                    self.error(
                        RuleCode::S014,
                        "a `Date` cannot be interpolated into a template; \
                         format it with `toISOString()` (Q20)",
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
                if sig.is_generator {
                    self.error(
                        RuleCode::S100,
                        "generators may only be called, not passed as values",
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
                    format!("generic function `{}` requires explicit type arguments", name),
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
                    self.error(
                        RuleCode::S002,
                        "no dynamic code evaluation",
                        pos.clone(),
                    );
                    self.err_expr(pos)
                } else if name == "Math" {
                    // The ambient namespace is not a value (Q19): it
                    // cannot be assigned, passed, or stored.
                    self.error(
                        RuleCode::S014,
                        "`Math` is an ambient namespace, not a value; \
                         only `Math.<member>` is accepted (Q19)",
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
                    "the `delete` operator is not in the language; use `unsafeDelete`",
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
                let numeric_ctx =
                    |t: &Type| -> Option<Type> { t.is_numeric().then(|| t.clone()) };
                let (left, right);
                if literalish(&b.left) && !literalish(&b.right) {
                    let r = self.check_expr(&b.right, outer.as_ref(), fx);
                    let c = numeric_ctx(&r.ty).or(outer);
                    left = self.check_expr(&b.left, c.as_ref(), fx);
                    right = r;
                } else {
                    left = self.check_expr(&b.left, outer.as_ref(), fx);
                    let c = if literalish(&b.right) {
                        numeric_ctx(&left.ty).or(outer)
                    } else {
                        outer
                    };
                    right = self.check_expr(&b.right, c.as_ref(), fx);
                }
                self.bin_result(b.op, left, right, pos)
            }
        }
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
                let hop = if op == B::EqEqEq { BinOp::Eq } else { BinOp::Ne };
                let null_cmp = matches!(
                    (&lt, &rt),
                    (Type::Null, Type::Nullable(_))
                        | (Type::Nullable(_), Type::Null)
                        | (Type::Null, Type::Null)
                );
                let same_scalar = lt == rt
                    && (lt.is_numeric()
                        || matches!(lt, Type::Bool | Type::Str | Type::Enum(_))
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
        if ok
            && matches!(op, B::LShift | B::RShift | B::ZeroFillRShift)
            && matches!(&right.kind, ExprKind::Int(amount) if *amount >= integer_width(&lt).unwrap_or(i64::MAX))
        {
            let amount = match &right.kind {
                ExprKind::Int(amount) => *amount,
                _ => 0,
            };
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
            self.error(
                RuleCode::S014,
                "`Date` values are not directly comparable; compare `getTime()` \
                 values (Q20)",
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
        let then = self.check_expr(&c.cons, ctx, fx);
        let els = self.check_expr(&c.alt, ctx, fx);
        let then_ty = then.ty.clone();
        self.require_assignable(&els.ty.clone(), &then_ty, els.pos.clone(), "the else branch");
        hir::Expr {
            kind: ExprKind::Cond {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            ty: then_ty,
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
                && self.is_reference_class(&target));
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
        let mut elems: Vec<&ast::ExprOrSpread> = Vec::new();
        for e in &a.elems {
            match e {
                Some(e) if e.spread.is_none() => elems.push(e),
                Some(e) => {
                    let p = self.pos(e.spread.unwrap_or(a.span));
                    let spread = self.check_expr(&e.expr, None, fx);
                    if matches!(spread.ty, Type::Map(..) | Type::Set(_)) {
                        self.error(
                            RuleCode::S014,
                            "spreading Map/Set requires the rejected iterator \
                             protocol; use `forEach` (Q24)",
                            p,
                        );
                    } else {
                        self.error(RuleCode::S100, "spread elements are not decided", p);
                    }
                }
                None => {
                    self.error(
                        RuleCode::S100,
                        "array holes are not decided",
                        pos.clone(),
                    );
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
                format!("`{}` may be null here; narrow with a null check first", name),
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
        let ast::Expr::Ident(id) = obj else { return None };
        let name = id.sym.to_string();
        // A local binding shadows any type name.
        let is_local = fx
            .scopes
            .iter()
            .rev()
            .any(|s| s.vars.contains_key(&name));
        if is_local {
            return None;
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
        // `Date.<member>` (stdlib.md §3): the static function members
        // (`UTC`, `now`) are intercepted by `check_method_call` before
        // this point; here every member read is a rejection.
        if name == "Date" && self.scope_item(&name).is_none() {
            return Some(self.check_date_member(prop, prop_pos, for_write));
        }
        if (name == "Map" || name == "Set") && self.scope_item(&name).is_none() {
            self.error(
                RuleCode::S014,
                format!(
                    "`{name}.{prop}` is outside the accepted Map/Set subset; \
                     static `groupBy` and iterator-based APIs are rejected (Q24)"
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
            Some(ScopeItem::Global(_)) | Some(ScopeItem::Func(_))
            | Some(ScopeItem::GenericFunc(_)) | Some(ScopeItem::Foreign(_)) => None,
            None => {
                if name == "Object" && prop == "setPrototypeOf" {
                    self.error(RuleCode::S003, "no prototype mutation", prop_pos.clone());
                    return Some(self.err_expr(prop_pos));
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
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|s| s.vars.contains_key("Math"));
        !shadowed && self.scope_item("Math").is_none()
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
        let why = match prop {
            "imul" | "clz32" | "fround" => {
                format!(
                    "`Math.{}`: JS-number op; the language has sized integers (Q19)",
                    prop
                )
            }
            _ => format!("`Math.{}` is outside the accepted Math subset (Q19)", prop),
        };
        self.error(RuleCode::S014, why, prop_pos.clone());
        self.err_expr(prop_pos)
    }

    /// A `Math.<fn>(…)` intrinsic call (stdlib.md §1): exact arity
    /// (Q19 — the lib's variadic `max`/`min`/`hypot` beyond two are out
    /// of subset), every argument `f64`, result `f64`.
    fn check_math_call(
        &mut self,
        f: MathFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let arity = f.arity();
        if c.args.len() != arity {
            self.error(
                RuleCode::S014,
                format!(
                    "`Math.{}` takes exactly {} f64 argument(s), got {} \
                     (Q19: the lib's variadic forms are out of subset)",
                    f.name(),
                    arity,
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let params: Vec<ParamSig> = (0..arity)
            .map(|_| ParamSig {
                name: String::new(),
                ty: Type::F64,
                has_default: false,
            })
            .collect();
        let args = self.check_args(&params, &c.args, fx, &pos, &format!("Math.{}", f.name()));
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Math(f),
                args,
            },
            ty: Type::F64,
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
    /// fold to f64 literals; predicates are call-only; aliases and all
    /// other members are rejected under Q25.
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
        if crate::ambient::number_predicate(prop).is_some() {
            self.error(
                RuleCode::S014,
                format!("`Number.{prop}` may only be called, not read as a value (Q25)"),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        let why = match prop {
            "parseInt" | "parseFloat" => format!(
                "`Number.{prop}` is rejected; use the global `{prop}` spelling (Q25)"
            ),
            _ => format!("`Number.{prop}` is outside the accepted Number subset (Q25)"),
        };
        self.error(RuleCode::S014, why, prop_pos.clone());
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

    /// Global `parseInt` / `parseFloat` calls. Their arity is part of
    /// Q25: in particular, parseInt's radix is required.
    fn check_number_global_call(
        &mut self,
        f: NumFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
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
            let why = if f == NumFn::ParseInt && c.args.len() == 1 {
                "`parseInt` requires an explicit radix (2–36, Q25)".to_string()
            } else {
                format!(
                    "`{}` takes exactly {} argument(s), got {} (Q25)",
                    f.name(),
                    params.len(),
                    c.args.len()
                )
            };
            self.error(RuleCode::S014, why, pos.clone());
            return self.err_expr(pos);
        }
        let args = self.check_args(&params, &c.args, fx, &pos, f.name());
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
        let shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|s| s.vars.contains_key("Date"));
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
        let why = match prop {
            "UTC" | "now" => format!(
                "`Date.{}` may only be called, not read as a value (Q20)",
                prop
            ),
            "parse" => "`Date.parse` is outside the accepted Date subset; construct \
                        with `Date.UTC(…)` (Q20)"
                .to_string(),
            _ => format!("`Date.{}` is outside the accepted Date subset (Q20)", prop),
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
                self.error(
                    RuleCode::S014,
                    "`new Date()` means the current time in the lib; out of subset — \
                     write `new Date(Date.now())` (Q20)",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                self.error(
                    RuleCode::S014,
                    "the multi-argument `new Date(y, m, …)` is interpreted in local \
                     time by the lib; out of subset — write `new Date(Date.UTC(y, m, …))` \
                     (Q20)",
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
        const LOCAL_ACCESSORS: &[&str] = &[
            "getFullYear",
            "getMonth",
            "getDate",
            "getDay",
            "getHours",
            "getMinutes",
            "getSeconds",
            "getMilliseconds",
            "getTimezoneOffset",
            "getYear",
        ];
        const TO_STRING_FAMILY: &[&str] = &[
            "toString",
            "toDateString",
            "toTimeString",
            "toLocaleString",
            "toLocaleDateString",
            "toLocaleTimeString",
            "toUTCString",
            "toJSON",
            "valueOf",
        ];
        let why = if LOCAL_ACCESSORS.contains(&name) {
            format!(
                "`{}` reads local time; the accepted Date subset is UTC-only — \
                 use the `getUTC…` accessor (Q20)",
                name
            )
        } else if name.starts_with("set") {
            format!(
                "`{}`: a `Date` is an immutable value; setters are out of subset — \
                 construct a new `Date` (Q20)",
                name
            )
        } else if TO_STRING_FAMILY.contains(&name) {
            format!(
                "`{}` is outside the accepted Date subset; format with \
                 `toISOString()` (Q20)",
                name
            )
        } else {
            format!("`{}` is outside the accepted Date subset (Q20)", name)
        };
        self.error(RuleCode::S014, why, pos);
    }

    /// A Q25 numeric receiver method. `toFixed` is accepted on
    /// `f32`/`f64`; an f32 receiver is widened exactly in HIR so the
    /// shared runtime has one f64 entry. Other Number formatting
    /// methods and integer receivers are rejected.
    fn check_number_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        if name == "toFixed" {
            if !matches!(&recv.ty, Type::F32 | Type::F64) {
                self.error(
                    RuleCode::S014,
                    "`toFixed` is accepted only on `f32`/`f64` (Q25)",
                    prop_pos,
                );
                return self.err_expr(pos);
            }
            if c.args.len() != 1 {
                self.error(
                    RuleCode::S014,
                    format!(
                        "`toFixed` takes exactly 1 i32 digit count, got {} (Q25)",
                        c.args.len()
                    ),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let digits = self.check_args(
                &[ParamSig {
                    name: String::new(),
                    ty: Type::I32,
                    has_default: false,
                }],
                &c.args,
                fx,
                &pos,
                "toFixed",
            );
            let recv_pos = recv.pos.clone();
            let recv = if recv.ty == Type::F32 {
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
            args.extend(digits);
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Num(NumFn::ToFixed),
                    args,
                },
                ty: Type::Str,
                pos,
            };
        }
        if matches!(
            name,
            "toPrecision" | "toExponential" | "toLocaleString" | "toString"
        ) {
            let why = if name == "toString" {
                "`toString(radix)` is rejected; use Q14 template interpolation \
                 for base 10 (Q25)"
                    .to_string()
            } else {
                format!("`{name}` is outside the accepted Number formatting subset (Q25)")
            };
            self.error(RuleCode::S014, why, prop_pos);
            return self.err_expr(pos);
        }
        let type_name = self.type_name(&recv.ty);
        self.error(
            RuleCode::S100,
            format!("`{type_name}` has no method `{name}`"),
            prop_pos,
        );
        self.err_expr(pos)
    }

    /// A `String` method intrinsic call on a string receiver
    /// (stdlib.md §8, Q21). Optional arguments are normalized here —
    /// `from` defaults to `0`, `pad` to `" "` — so every runtime symbol
    /// has a fixed arity and both tiers lower the identical call (the
    /// Date.UTC technique, §3). The receiver becomes the call's first
    /// argument.
    fn check_str_method(
        &mut self,
        recv: hir::Expr,
        f: StrFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let optional_from = matches!(f, StrFn::IndexOf | StrFn::Includes);
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
                has_default: i == 1 && (optional_from || optional_pad),
            })
            .collect();
        let mut args = self.check_args(&params, &c.args, fx, &pos, f.name());
        if args.len() + 1 == params.len() {
            if optional_from {
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
        const REDUNDANT_WITH_SLICE: &[&str] = &["substring", "substr", "at", "charAt"];
        const LOCALE_OR_UNICODE: &[&str] = &[
            "localeCompare",
            "toLocaleUpperCase",
            "toLocaleLowerCase",
            "normalize",
        ];
        const REGEX: &[&str] = &["match", "matchAll", "search"];
        let why = if REDUNDANT_WITH_SLICE.contains(&name) {
            format!(
                "`{}` is redundant with the byte-measure `slice`; out of the \
                 accepted String subset (Q21)",
                name
            )
        } else if LOCALE_OR_UNICODE.contains(&name) {
            format!(
                "`{}` is locale- or Unicode-table-dependent; the accepted String \
                 subset is ASCII/byte-based (Q21)",
                name
            )
        } else if REGEX.contains(&name) {
            format!("`{}` requires RegExp, a stdlib non-goal (Q21)", name)
        } else if name == "concat" {
            "`concat` is redundant with `+` concatenation; out of the accepted \
             String subset (Q21)"
                .to_string()
        } else if name == "codePointAt" {
            "`codePointAt` is outside the accepted String subset; `charCodeAt` \
             reads byte values (Q21)"
                .to_string()
        } else {
            return false;
        };
        self.error(RuleCode::S014, why, pos);
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
        let needs_elem_kind = f.takes_callback() || matches!(f, A::IndexOf | A::LastIndexOf | A::Includes);
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
                let ty = if f == A::Includes { Type::Bool } else { Type::I32 };
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
            A::Sort => {
                if c.args.is_empty() {
                    self.error(
                        RuleCode::S014,
                        "`sort` requires a comparator: the lib's no-argument sort \
                         coerces elements to strings (Q22)",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!("`sort` expects 1 argument (the comparator), got {}", c.args.len()),
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
                );
                mk(vec![recv, cb], arr_ty, pos)
            }
            A::Reduce => {
                if c.args.len() < 2 {
                    self.error(
                        RuleCode::S014,
                        "`reduce` requires an explicit `init`: the lib's no-init \
                         overload changes meaning by arity (Q22)",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args.len() != 2 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`reduce` expects 2 arguments (callback, init), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if let Some(spread) = c.args[1].spread {
                    let p = self.pos(spread);
                    self.error(RuleCode::S100, "spread arguments are not decided", p.clone());
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
                        self.require_assignable(&init.ty, u, init.pos.clone(), "the `reduce` init");
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
                            "the `reduce` accumulator crosses the runtime↔script \
                             boundary; `{}` is outside the supported kinds (Q22)",
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
                        "reduce",
                    ),
                    None => self.check_arr_callback(
                        &c.args[0],
                        vec![acc_ty.clone(), elem],
                        Some(acc_ty.clone()),
                        fx,
                        "reduce",
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
                    self.check_arr_callback(&c.args[0], vec![elem], ret_ctx, fx, f.name());
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
            || matches!(value, Type::Nullable(inner) if self.is_reference_class(inner))
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
        let map_ty = Type::Map(Box::new(key.clone()), Box::new(value.clone()));
        let mk = |f: MapFn, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Map(f),
                args,
            },
            ty,
            pos,
        };
        match name {
            "get" => {
                if !self.map_get_value_ok(&value) {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`get` cannot report a miss for scalar-valued `{}`; \
                             use `has` plus `getOr` (Q24)",
                            self.type_name(&map_ty)
                        ),
                        prop_pos,
                    );
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
            "getOr" => {
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
            "set" => {
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
            "has" | "delete" => {
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
                    if name == "has" {
                        "Map.has"
                    } else {
                        "Map.delete"
                    },
                ));
                mk(
                    if name == "has" {
                        MapFn::Has
                    } else {
                        MapFn::Delete
                    },
                    args,
                    Type::Bool,
                    pos,
                )
            }
            "clear" => {
                let checked = self.check_args(&[], &c.args, fx, &pos, "Map.clear");
                let mut args = vec![recv];
                args.extend(checked);
                mk(MapFn::Clear, args, Type::Void, pos)
            }
            "forEach" => {
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
                );
                mk(MapFn::ForEach, vec![recv, callback], Type::Void, pos)
            }
            "keys" | "values" | "entries" => {
                self.error(
                    RuleCode::S014,
                    format!("`Map.{name}` requires the iterator protocol; use `forEach` (Q24)"),
                    prop_pos,
                );
                self.err_expr(pos)
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    format!("`Map` has no accepted method `{name}` (Q24)"),
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
        let set_ty = Type::Set(Box::new(key.clone()));
        let mk = |f: SetFn, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Set(f),
                args,
            },
            ty,
            pos,
        };
        match name {
            "add" | "has" | "delete" => {
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
                    &format!("Set.{name}"),
                ));
                let (f, ty) = match name {
                    "add" => (SetFn::Add, set_ty),
                    "has" => (SetFn::Has, Type::Bool),
                    _ => (SetFn::Delete, Type::Bool),
                };
                mk(f, args, ty, pos)
            }
            "clear" => {
                let checked = self.check_args(&[], &c.args, fx, &pos, "Set.clear");
                let mut args = vec![recv];
                args.extend(checked);
                mk(SetFn::Clear, args, Type::Void, pos)
            }
            "forEach" => {
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
                );
                mk(SetFn::ForEach, vec![recv, callback], Type::Void, pos)
            }
            "keys"
            | "values"
            | "entries"
            | "union"
            | "intersection"
            | "difference"
            | "symmetricDifference"
            | "isSubsetOf"
            | "isSupersetOf"
            | "isDisjointFrom" => {
                self.error(
                    RuleCode::S014,
                    format!("`Set.{name}` is outside the accepted Q24 subset"),
                    prop_pos,
                );
                self.err_expr(pos)
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    format!("`Set` has no accepted method `{name}` (Q24)"),
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

    /// Checks one callback argument of an `Array` method (stdlib.md §9):
    /// a lambda is typed with the fixed parameter context (its arity
    /// must match exactly — the lib's optional index/array parameters
    /// are rejected, Q22); any other expression must already have the
    /// exact function type (a named function reference or a
    /// function-typed local, C5). `ret` is `None` when the return type
    /// is inferred from the callback (`map`).
    fn check_arr_callback(
        &mut self,
        arg: &ast::ExprOrSpread,
        params: Vec<Type>,
        ret: Option<Type>,
        fx: &mut FnCtx,
        method: &str,
    ) -> hir::Expr {
        if let Some(spread) = arg.spread {
            let p = self.pos(spread);
            self.error(RuleCode::S100, "spread arguments are not decided", p.clone());
            return self.err_expr(p);
        }
        let expr = &*arg.expr;
        if let ast::Expr::Arrow(a) = expr {
            let a_pos = self.pos(a.span);
            if a.params.len() != params.len() {
                let q24 = method.starts_with("Map.") || method.starts_with("Set.");
                self.error(
                    RuleCode::S014,
                    if q24 {
                        format!(
                            "`{method}` callbacks take exactly {} parameter(s); \
                             extra lib callback parameters are not accepted (Q24)",
                            params.len()
                        )
                    } else {
                        format!(
                            "`{method}` callbacks take exactly {} parameter(s); the \
                             lib's optional index/array parameters are not accepted (Q22)",
                            params.len()
                        )
                    },
                    a_pos.clone(),
                );
                return self.err_expr(a_pos);
            }
            let checked = self.check_lambda_with(a, Some(&params), ret.as_ref(), fx, a_pos);
            // An annotation may override the context; the resulting
            // function type must still match the method's fixed shape
            // (the return stays free when it is inferred, `map`).
            return self.expect_callback_shape(checked, &params, ret.as_ref(), method);
        }
        // A function value (named reference or function-typed local).
        let ctx_ty = ret.as_ref().map(|r| {
            Type::Func(Box::new(FuncType {
                params: params.clone(),
                ret: r.clone(),
            }))
        });
        let checked = self.check_expr(expr, ctx_ty.as_ref(), fx);
        self.expect_callback_shape(checked, &params, ret.as_ref(), method)
    }

    /// Validates a checked callback value against the method's fixed
    /// parameter list (and return type, when it is not inferred);
    /// returns the value unchanged on success and a poisoned expression
    /// after the mismatch diagnostic otherwise.
    fn expect_callback_shape(
        &mut self,
        checked: hir::Expr,
        params: &[Type],
        ret: Option<&Type>,
        method: &str,
    ) -> hir::Expr {
        let ok = match &checked.ty {
            Type::Error => true,
            Type::Func(ft) => {
                ft.params == params && ret.is_none_or(|r| ft.ret == *r)
            }
            _ => false,
        };
        if ok {
            return checked;
        }
        let got = self.type_name(&checked.ty);
        let wanted: Vec<String> = params.iter().map(|t| self.type_name(t)).collect();
        let ret_n = match ret {
            Some(r) => self.type_name(r),
            None => "…".to_string(),
        };
        self.error(
            RuleCode::S100,
            format!(
                "type mismatch: the `{}` callback expects `({}) => {}`, got `{}`",
                method,
                wanted.join(", "),
                ret_n,
                got
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
        const STRUCTURAL: &[&str] = &["splice", "shift", "unshift", "copyWithin"];
        const NESTING: &[&str] = &["flat", "flatMap"];
        const ITERATORS: &[&str] = &["entries", "keys", "values"];
        let why = if name == "find" || name == "findLast" {
            format!(
                "`{}` has no miss value for scalar element types (`T | null` does \
                 not cover scalars); use `findIndex` (Q22)",
                name
            )
        } else if name == "reduceRight" {
            "`reduceRight` is outside the accepted Array subset; fold with `reduce` \
             (Q22)"
                .to_string()
        } else if STRUCTURAL.contains(&name) {
            format!(
                "`{}` is outside the accepted Array subset (push, pop, slice, fill, \
                 and the Q22 methods) (Q22)",
                name
            )
        } else if NESTING.contains(&name) {
            format!("`{}` requires nested-array flattening, out of the Q22 subset (Q22)", name)
        } else if ITERATORS.contains(&name) {
            format!("`{}` requires the iterator protocol, out of the Q22 subset (Q22)", name)
        } else {
            return false;
        };
        self.error(RuleCode::S014, why, pos);
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
        let pos = self.pos(m.span);
        match &m.prop {
            ast::MemberProp::Computed(c) => {
                let obj = self.check_receiver(&m.obj, fx);
                let index = self.check_expr(&c.expr, Some(&Type::I32), fx);
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

    fn check_index(&mut self, obj: hir::Expr, index: hir::Expr, pos: Pos) -> hir::Expr {
        if !matches!(index.ty, Type::I32 | Type::Error) {
            let name = self.type_name(&index.ty);
            self.error(
                RuleCode::S100,
                format!("array indices are `i32`, got `{}`", name),
                index.pos.clone(),
            );
        }
        let elem = match &obj.ty {
            Type::Array(t) => (**t).clone(),
            Type::FixedArray(t, n) => {
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
                } else if self.class_sigs[id.0].methods.contains_key(name) {
                    self.error(
                        RuleCode::S100,
                        format!("method `{}` may only be called, not read as a value", name),
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
                            format!(
                                "method `{}` may only be called, not read as a value",
                                name
                            ),
                            prop_pos.clone(),
                        );
                    } else if !self.arr_subset_rejection(name, prop_pos.clone()) {
                        self.arr_surface_error(name, prop_pos.clone());
                    }
                } else {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` is outside the FixedArray surface (length, indexing)",
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
                if !for_write
                    && matches!(
                        name,
                        "get" | "getOr" | "set" | "has" | "delete" | "clear" | "forEach"
                    )
                {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` may only be called, not read as a value"),
                        prop_pos.clone(),
                    );
                } else if matches!(name, "keys" | "values" | "entries") {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`{name}` requires the iterator protocol; use `forEach` (Q24)"
                        ),
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
                if !for_write
                    && matches!(name, "add" | "has" | "delete" | "clear" | "forEach")
                {
                    self.error(
                        RuleCode::S100,
                        format!("method `{name}` may only be called, not read as a value"),
                        prop_pos.clone(),
                    );
                } else if matches!(
                    name,
                    "keys"
                        | "values"
                        | "entries"
                        | "union"
                        | "intersection"
                        | "difference"
                        | "symmetricDifference"
                        | "isSubsetOf"
                        | "isSupersetOf"
                        | "isDisjointFrom"
                ) {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`{name}` is outside the accepted Set subset; use `forEach` \
                             for traversal (Q24)"
                        ),
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
                if !for_write
                    && (name == "slice" || crate::ambient::str_method(name).is_some())
                {
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
                        format!("`Date` is an immutable value; `{}` cannot be assigned (Q20)", name),
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
                    "toFixed"
                        | "toPrecision"
                        | "toExponential"
                        | "toLocaleString"
                        | "toString"
                );
                if known {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "numeric method `{name}` may only appear in an accepted call \
                             (`toFixed` on f32/f64; Q25)"
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

    fn check_assign(&mut self, a: &ast::AssignExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
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
                BinOp::Add => {
                    target_ty.is_numeric() || target_ty == Type::Str
                }
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
            if matches!(bin, BinOp::Shl | BinOp::Shr | BinOp::UShr)
                && matches!(
                    &value.kind,
                    ExprKind::Int(amount)
                        if *amount >= integer_width(&target_ty).unwrap_or(i64::MAX)
                )
            {
                let amount = match &value.kind {
                    ExprKind::Int(amount) => *amount,
                    _ => 0,
                };
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
            fx.narrowed
                .retain(|k| k != &key && !k.starts_with(&prefix));
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
                let index = self.check_expr(&c.expr, Some(&Type::I32), fx);
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

    fn check_call(&mut self, c: &ast::CallExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
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
            ast::Expr::Ident(id) => self.check_named_call(id, c, fx, pos),
            ast::Expr::Member(m) => self.check_method_call(m, c, fx, pos),
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
            None => {
                if name == "eval" {
                    self.error(RuleCode::S002, "no dynamic code evaluation", pos.clone());
                    return self.err_expr(pos);
                }
                if let Some(f) = crate::ambient::number_global(&name) {
                    return self.check_number_global_call(f, c, fx, pos);
                }
                if name == "Number" {
                    self.error(
                        RuleCode::S014,
                        "`Number(x)` coercion is rejected; use explicit `as` conversion (Q25)",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if name == "isNaN" || name == "isFinite" {
                    self.error(
                        RuleCode::S014,
                        format!(
                            "the coercing global `{name}` is rejected; use `Number.{name}` (Q25)"
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if let Some(ambient) = crate::ambient::ambient_fn(&name) {
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
            AmbientFn::Collect => "collect",
            AmbientFn::UnsafeDelete => "unsafeDelete",
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
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let ast::MemberProp::Ident(prop) = &m.prop else {
            let value = self.check_member_read(m, fx);
            return self.check_indirect_call(value, c, fx, pos);
        };
        let name = prop.sym.to_string();
        let prop_pos = self.pos(prop.span);
        // `Math.<fn>(…)` (stdlib.md §1): an ambient-namespace intrinsic
        // call, resolved before the generic namespace-member path (which
        // treats a function member read as an error). Constants and
        // out-of-subset members fall through to that path.
        if self.is_math_namespace(&m.obj, fx) {
            if let Some(f) = crate::ambient::math_fn(&name) {
                return self.check_math_call(f, c, fx, pos);
            }
        }
        // `Number.is*` (stdlib.md §11.1): accepted predicates are
        // resolved before generic namespace-member handling.
        if self.is_number_namespace(&m.obj, fx) {
            if let Some(f) = crate::ambient::number_predicate(&name) {
                return self.check_number_predicate_call(f, c, fx, pos);
            }
        }
        // `Date.UTC(…)` / `Date.now()` (stdlib.md §3): static intrinsic
        // calls, resolved before the generic namespace-member path.
        if self.is_date_namespace(&m.obj, fx) {
            if let Some(handled) = self.check_date_static_call(&name, c, fx, pos.clone()) {
                return handled;
            }
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
            ty if ty.is_numeric() => {
                self.check_number_method(recv, &name, c, fx, pos, prop_pos)
            }
            Type::Date => self.check_date_method(recv, &name, c, fx, pos, prop_pos),
            Type::Map(key, value) => {
                self.check_map_method(recv, *key, *value, &name, c, fx, pos, prop_pos)
            }
            Type::Set(key) => {
                self.check_set_method(recv, *key, &name, c, fx, pos, prop_pos)
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
            // stdlib.md §9: the v1 Array methods are `T[]` only. Only the
            // Q22 methods cite Q22 — `push`/`pop` are not Q22 members
            // (`ambient::arr_method` excludes them), so they keep the
            // standing "no method" diagnostic of the fall-through arm.
            Type::FixedArray(..) if crate::ambient::arr_method(&name).is_some() => {
                self.error(
                    RuleCode::S014,
                    format!(
                        "`{}` is not available on `FixedArray`; the Array methods \
                         apply to `T[]` only (Q22)",
                        name
                    ),
                    prop_pos.clone(),
                );
                self.err_expr(pos)
            }
            Type::Str => match name.as_str() {
                "slice" => {
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
                    let args = self.check_args(&params, &c.args, fx, &pos, "slice");
                    mk(recv, args, Type::Str, pos)
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
            Type::Generator(y) => match name.as_str() {
                "next" => {
                    let args = self.check_args(&[], &c.args, fx, &pos, "next");
                    mk(recv, args, Type::IterResult(y.clone()), pos)
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
        if args.len() < required || args.len() > params.len() {
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
                self.error(RuleCode::S100, "spread arguments are not decided", p);
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
            self.error(
                RuleCode::S100,
                "`new` requires a class name",
                pos.clone(),
            );
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
                "`Promise` requires an event loop; the language has none",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "Number" && self.is_number_namespace(callee, fx) {
            self.error(
                RuleCode::S014,
                "`new Number(x)` boxing/coercion is rejected; use an explicit sized \
                 numeric type and `as` conversion (Q25)",
                pos.clone(),
            );
            return self.err_expr(pos);
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
                self.error(
                    RuleCode::S014,
                    format!(
                        "`new {name}(iterable)` is rejected; iterable construction \
                         requires the iterator protocol (Q24)"
                    ),
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
                        format!(
                            "generic class `{}` requires explicit type arguments",
                            name
                        ),
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
                RuleCode::S013,
                "`async` requires an event loop; the language has none",
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
                        self.error(
                            RuleCode::S100,
                            "not all paths return a value",
                            pos.clone(),
                        );
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
