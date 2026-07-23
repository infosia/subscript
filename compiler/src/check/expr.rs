//! Expression checking: contextual literal typing (C4), sized-numeric
//! arithmetic (C3/Q18), nominal member access (C1/Q4/Q5), calls, `as`
//! conversions, lambdas (C5), and null narrowing at use sites (C7).

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Pos, RuleCode};
use crate::hir::{self, AmbientFn, BinOp, Callee, ExprKind, TplPart, UnOp};
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
        Type::I32 => Some((i64::from(i32::MIN), i64::from(i32::MAX))),
        Type::U32 => Some((0, i64::from(u32::MAX))),
        Type::I64 => Some((-EXACT, EXACT)),
        Type::U64 => Some((0, EXACT)),
        _ => None,
    }
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
                if !printable {
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
                if name == "eval" || name == "Function" {
                    self.error(
                        RuleCode::S002,
                        "no dynamic code evaluation",
                        pos.clone(),
                    );
                } else if crate::ambient::ambient_fn(&name).is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("ambient function `{}` may only be called", name),
                        pos.clone(),
                    );
                } else {
                    self.error(
                        RuleCode::S100,
                        format!("unknown name `{}`", name),
                        pos.clone(),
                    );
                }
                self.err_expr(pos)
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
        if ok || err {
            return mk(hop, if err { Type::Error } else { ty });
        }
        let ln = self.type_name(&lt);
        let rn = self.type_name(&rt);
        if mixed_numeric {
            self.error(
                RuleCode::S007,
                format!(
                    "mixed-type arithmetic (`{}` and `{}`) requires an explicit `as` conversion",
                    ln, rn
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
            || (src.is_numeric() && target.is_numeric())
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
                    self.error(RuleCode::S100, "spread elements are not decided", p);
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
    /// namespace (enums, class statics, `Object.setPrototypeOf`).
    /// Returns `Some` when handled.
    fn check_namespace_member(
        &mut self,
        obj: &ast::Expr,
        prop: &str,
        prop_pos: Pos,
        fx: &mut FnCtx,
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
                if let Some(handled) = self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx)
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
                let surface = if matches!(obj.ty, Type::Array(_)) {
                    "the array surface (length, indexing, push, pop)"
                } else {
                    "the FixedArray surface (length, indexing)"
                };
                self.error(
                    RuleCode::S100,
                    format!("`{}` is outside {}", name, surface),
                    prop_pos.clone(),
                );
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
                self.error(
                    RuleCode::S100,
                    format!(
                        "`{}` is outside the string surface (length, slice, `+` \
                         concatenation, `===`/`!==`)",
                        name
                    ),
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
                if let Some(handled) = self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx)
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
        if let Some(handled) = self.check_namespace_member(&m.obj, &name, prop_pos.clone(), fx) {
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
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` is outside the array surface (length, indexing, push, pop)",
                            other
                        ),
                        prop_pos.clone(),
                    );
                    self.err_expr(pos)
                }
            },
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
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` is outside the string surface (length, slice, `+` \
                             concatenation, `===`/`!==`)",
                            other
                        ),
                        prop_pos.clone(),
                    );
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
        let ctx_fn = match ctx {
            Some(Type::Func(ft)) => Some((**ft).clone()),
            _ => None,
        };
        let mut params = Vec::new();
        for (i, pat) in a.params.iter().enumerate() {
            // An un-annotated lambda parameter takes its type from the
            // contextual function type (tsc-style contextual typing);
            // only a parameter with neither annotation nor context is an
            // error. This is how a boundary callback (e.g. a `void*`
            // `object | null` userdata slot) is typed without the program
            // spelling the boundary type itself.
            let unannotated = matches!(
                pat,
                ast::Pat::Ident(b) if b.type_ann.is_none() && !b.id.optional
            );
            let sig = if unannotated {
                if let Some(t) = ctx_fn.as_ref().and_then(|f| f.params.get(i)) {
                    let ast::Pat::Ident(b) = pat else { unreachable!() };
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
            .or_else(|| ctx_fn.as_ref().map(|f| f.ret.clone()));

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
