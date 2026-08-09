//! HIR trap-site decisions that require control-flow facts.
//!
//! Most sites are a direct property of an [`Expr`](crate::hir::Expr) and
//! are exposed by `Expr::trap_sites`. Fixed-array bounds-check elision is
//! the exception: it depends on the enclosing counted loop. This pass runs
//! once after checking and records that decision on each `Index` node so
//! neither lowering owns an interval analysis.

use std::collections::HashMap;

use crate::hir::{self, ExprKind as K};
use crate::Type;

/// An inclusive integer interval `[lo, hi]`, widened through `i128` for
/// arithmetic so the proof lattice itself never wraps.
#[derive(Debug, Clone, Copy)]
struct Interval {
    lo: i64,
    hi: i64,
}

impl Interval {
    fn point(value: i64) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }

    fn fit(lo: i128, hi: i128) -> Option<Self> {
        let lo = i64::try_from(lo).ok()?;
        let hi = i64::try_from(hi).ok()?;
        (lo <= hi).then_some(Self { lo, hi })
    }

    fn add(self, other: Self) -> Option<Self> {
        Self::fit(
            i128::from(self.lo) + i128::from(other.lo),
            i128::from(self.hi) + i128::from(other.hi),
        )
    }

    fn sub(self, other: Self) -> Option<Self> {
        Self::fit(
            i128::from(self.lo) - i128::from(other.hi),
            i128::from(self.hi) - i128::from(other.lo),
        )
    }

    fn mul(self, other: Self) -> Option<Self> {
        let products = [
            i128::from(self.lo) * i128::from(other.lo),
            i128::from(self.lo) * i128::from(other.hi),
            i128::from(self.hi) * i128::from(other.lo),
            i128::from(self.hi) * i128::from(other.hi),
        ];
        Self::fit(
            products.iter().copied().min()?,
            products.iter().copied().max()?,
        )
    }
}

fn int_type_range(ty: &Type) -> Option<Interval> {
    Some(match ty {
        Type::I8 => Interval {
            lo: i64::from(i8::MIN),
            hi: i64::from(i8::MAX),
        },
        Type::U8 => Interval {
            lo: 0,
            hi: i64::from(u8::MAX),
        },
        Type::I16 => Interval {
            lo: i64::from(i16::MIN),
            hi: i64::from(i16::MAX),
        },
        Type::U16 => Interval {
            lo: 0,
            hi: i64::from(u16::MAX),
        },
        Type::I32 => Interval {
            lo: i64::from(i32::MIN),
            hi: i64::from(i32::MAX),
        },
        Type::U32 => Interval {
            lo: 0,
            hi: i64::from(u32::MAX),
        },
        Type::I64 => Interval {
            lo: i64::MIN,
            hi: i64::MAX,
        },
        // The lattice is i64-valued, so u64's upper bound is not
        // representable and cannot support a proof.
        Type::U64 => return None,
        _ => return None,
    })
}

pub(crate) fn decide_index_checks(module: &mut hir::Module) {
    for class in &mut module.classes {
        for field in &mut class.fields {
            if let Some(init) = &mut field.init {
                Analyzer::default().expr(init);
            }
        }
        if let Some(ctor) = &mut class.ctor {
            Analyzer::default().function(ctor);
        }
        for method in &mut class.methods {
            Analyzer::default().function(method);
        }
    }
    for global in &mut module.globals {
        Analyzer::default().expr(&mut global.init);
    }
    for function in &mut module.functions {
        Analyzer::default().function(function);
    }
    Analyzer::default().stmts(&mut module.top_level);
}

#[derive(Default)]
struct Analyzer {
    ranges: HashMap<String, Interval>,
}

impl Analyzer {
    fn function(&mut self, function: &mut hir::Function) {
        for param in &mut function.params {
            if let Some(default) = &mut param.default {
                self.expr(default);
            }
        }
        self.stmts(&mut function.body);
    }

    fn stmts(&mut self, stmts: &mut [hir::Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn scoped_stmts(&mut self, stmts: &mut [hir::Stmt]) {
        let saved = self.ranges.clone();
        self.stmts(stmts);
        self.ranges = saved;
    }

    fn stmt(&mut self, stmt: &mut hir::Stmt) {
        match stmt {
            hir::Stmt::Let { name, init, .. } => {
                self.expr(init);
                // A same-named binding shadows any proven outer counter.
                self.ranges.remove(name);
            }
            hir::Stmt::Expr(expr) => self.expr(expr),
            hir::Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                self.expr(cond);
                self.scoped_stmts(then);
                if let Some(els) = els {
                    self.scoped_stmts(els);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.scoped_stmts(body);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let saved = self.ranges.clone();
                if let Some(init) = init {
                    self.stmt(init);
                }
                let proof =
                    self.induction_interval(init.as_deref(), cond.as_ref(), step.as_ref(), body);
                if let Some((name, range)) = proof {
                    self.ranges.insert(name, range);
                }
                if let Some(cond) = cond {
                    self.expr(cond);
                }
                self.scoped_stmts(body);
                if let Some(step) = step {
                    self.expr(step);
                }
                self.ranges = saved;
            }
            hir::Stmt::ForOf { subject, body, .. } => {
                self.expr(subject);
                self.scoped_stmts(body);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                self.expr(disc);
                for case in cases {
                    if let Some(test) = &mut case.test {
                        self.expr(test);
                    }
                    self.scoped_stmts(&mut case.body);
                }
            }
            hir::Stmt::Block(body) => self.scoped_stmts(body),
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
        }
    }

    fn expr(&mut self, expr: &mut hir::Expr) {
        match &mut expr.kind {
            K::Unary { operand, .. } => self.expr(operand),
            K::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            K::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            K::Cast(inner) => self.expr(inner),
            K::Call { callee, args } => {
                match callee {
                    hir::Callee::Value(value) => self.expr(value),
                    hir::Callee::Method { recv, .. } => self.expr(recv),
                    _ => {}
                }
                for arg in args {
                    self.expr(arg);
                }
            }
            K::AsyncCall { callee, args } => {
                if let Some(receiver) = callee.receiver_mut() {
                    self.expr(receiver);
                }
                for arg in args {
                    self.expr(arg);
                }
            }
            K::New { args, .. } => {
                for arg in args {
                    self.expr(arg);
                }
            }
            K::DescriptorLit { fields, .. } => {
                for value in fields.iter_mut().flatten() {
                    self.expr(value);
                }
            }
            K::Field { obj, .. } | K::JsonResultValue(obj) | K::Length(obj) => self.expr(obj),
            K::Index {
                obj,
                index,
                checked,
            } => {
                self.expr(obj);
                self.expr(index);
                *checked = match &obj.ty {
                    Type::FixedArray(_, len) => !self.index_in_bounds(index, *len),
                    // Dynamic arrays have no compile-time length proof.
                    _ => true,
                };
            }
            K::ArrayLit(elems) => {
                for elem in elems {
                    self.expr(elem);
                }
            }
            K::ArraySpreadLit(elems) => {
                for elem in elems {
                    self.expr(&mut elem.expr);
                }
            }
            K::Template(parts) => {
                for part in parts {
                    if let hir::TplPart::Expr(expr) = part {
                        self.expr(expr);
                    }
                }
            }
            K::Lambda { body, .. } => Analyzer::default().stmts(body),
            K::Yield(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            K::Cond { cond, then, els } => {
                self.expr(cond);
                self.expr(then);
                self.expr(els);
            }
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(_)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::RawNew { .. }
            | K::AsyncSuspend => {}
        }
    }

    fn interval_of(&self, expr: &hir::Expr) -> Option<Interval> {
        if !expr.ty.is_integer() {
            return None;
        }
        match &expr.kind {
            K::Int(value) => Some(Interval::point(*value)),
            K::EnumMember { value, .. } => Some(Interval::point(*value)),
            K::Local(name) => self.ranges.get(name).copied(),
            K::Length(obj) => match obj.ty {
                Type::FixedArray(_, len) => Some(Interval::point(i64::from(len))),
                _ => None,
            },
            K::Binary { op, left, right } => {
                let left = self.interval_of(left)?;
                let right = self.interval_of(right)?;
                match op {
                    hir::BinOp::Add => left.add(right),
                    hir::BinOp::Sub => left.sub(right),
                    hir::BinOp::Mul => left.mul(right),
                    _ => None,
                }
            }
            K::Cast(inner) => {
                if !inner.ty.is_integer() {
                    return None;
                }
                let interval = self.interval_of(inner)?;
                let target = int_type_range(&expr.ty)?;
                (interval.lo >= target.lo && interval.hi <= target.hi).then_some(interval)
            }
            _ => None,
        }
    }

    fn index_in_bounds(&self, index: &hir::Expr, len: u32) -> bool {
        self.interval_of(index)
            .is_some_and(|range| range.lo >= 0 && range.hi < i64::from(len))
    }

    fn induction_interval(
        &self,
        init: Option<&hir::Stmt>,
        cond: Option<&hir::Expr>,
        step: Option<&hir::Expr>,
        body: &[hir::Stmt],
    ) -> Option<(String, Interval)> {
        let (name, ty, start) = match init? {
            hir::Stmt::Let { name, ty, init, .. } if ty.is_integer() => {
                (name.clone(), ty, self.interval_of(init)?)
            }
            _ => return None,
        };
        if start.lo != start.hi {
            return None;
        }
        let (op, bound) = match &cond?.kind {
            K::Binary { op, left, right } => match &left.kind {
                K::Local(local) if *local == name => (*op, self.interval_of(right)?),
                _ => return None,
            },
            _ => return None,
        };
        let hi = match op {
            hir::BinOp::Lt => bound.hi.checked_sub(1)?,
            hir::BinOp::Le => bound.hi,
            _ => return None,
        };
        let increment = match &step?.kind {
            K::Assign {
                op: Some(hir::BinOp::Add),
                target,
                value,
            } => match &target.kind {
                K::Local(local) if *local == name => self.interval_of(value)?,
                _ => return None,
            },
            _ => return None,
        };
        if increment.lo != increment.hi || increment.lo <= 0 || start.lo > hi {
            return None;
        }
        let type_range = int_type_range(ty)?;
        let after_step = i128::from(hi) + i128::from(increment.lo);
        if after_step > i128::from(type_range.hi)
            || i128::from(start.lo) < i128::from(type_range.lo)
            || stmts_assign_to(body, &name)
        {
            return None;
        }
        Some((name, Interval { lo: start.lo, hi }))
    }
}

fn stmts_assign_to(stmts: &[hir::Stmt], name: &str) -> bool {
    stmts.iter().any(|stmt| stmt_assigns_to(stmt, name))
}

fn stmt_assigns_to(stmt: &hir::Stmt, name: &str) -> bool {
    match stmt {
        hir::Stmt::Let { init, .. } => expr_assigns_to(init, name),
        hir::Stmt::Expr(expr) => expr_assigns_to(expr, name),
        hir::Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|value| expr_assigns_to(value, name)),
        hir::Stmt::If {
            cond, then, els, ..
        } => {
            expr_assigns_to(cond, name)
                || stmts_assign_to(then, name)
                || els
                    .as_ref()
                    .is_some_and(|branch| stmts_assign_to(branch, name))
        }
        hir::Stmt::While { cond, body, .. } => {
            expr_assigns_to(cond, name) || stmts_assign_to(body, name)
        }
        hir::Stmt::For {
            init,
            cond,
            step,
            body,
            ..
        } => {
            init.as_deref()
                .is_some_and(|init| stmt_assigns_to(init, name))
                || cond
                    .as_ref()
                    .is_some_and(|cond| expr_assigns_to(cond, name))
                || step
                    .as_ref()
                    .is_some_and(|step| expr_assigns_to(step, name))
                || stmts_assign_to(body, name)
        }
        hir::Stmt::ForOf { subject, body, .. } => {
            expr_assigns_to(subject, name) || stmts_assign_to(body, name)
        }
        hir::Stmt::Switch { disc, cases, .. } => {
            expr_assigns_to(disc, name)
                || cases.iter().any(|case| {
                    case.test
                        .as_ref()
                        .is_some_and(|test| expr_assigns_to(test, name))
                        || stmts_assign_to(&case.body, name)
                })
        }
        hir::Stmt::Block(body) => stmts_assign_to(body, name),
        hir::Stmt::Break(_) | hir::Stmt::Continue(_) => false,
    }
}

fn expr_assigns_to(expr: &hir::Expr, name: &str) -> bool {
    match &expr.kind {
        K::Assign { target, value, .. } => {
            matches!(&target.kind, K::Local(local) if local == name)
                || expr_assigns_to(target, name)
                || expr_assigns_to(value, name)
        }
        K::Unary { operand, .. } => expr_assigns_to(operand, name),
        K::Binary { left, right, .. } => {
            expr_assigns_to(left, name) || expr_assigns_to(right, name)
        }
        K::Cast(inner) => expr_assigns_to(inner, name),
        K::Call { callee, args } => {
            let callee_assigns = match callee {
                hir::Callee::Value(value) => expr_assigns_to(value, name),
                hir::Callee::Method { recv, .. } => expr_assigns_to(recv, name),
                _ => false,
            };
            callee_assigns || args.iter().any(|arg| expr_assigns_to(arg, name))
        }
        K::AsyncCall { callee, args } => {
            callee
                .receiver()
                .is_some_and(|receiver| expr_assigns_to(receiver, name))
                || args.iter().any(|arg| expr_assigns_to(arg, name))
        }
        K::New { args, .. } => args.iter().any(|arg| expr_assigns_to(arg, name)),
        K::DescriptorLit { fields, .. } => fields
            .iter()
            .flatten()
            .any(|value| expr_assigns_to(value, name)),
        K::Field { obj, .. } | K::JsonResultValue(obj) | K::Length(obj) => {
            expr_assigns_to(obj, name)
        }
        K::Index { obj, index, .. } => expr_assigns_to(obj, name) || expr_assigns_to(index, name),
        K::ArrayLit(elems) => elems.iter().any(|elem| expr_assigns_to(elem, name)),
        K::ArraySpreadLit(elems) => elems.iter().any(|elem| expr_assigns_to(&elem.expr, name)),
        K::Template(parts) => parts.iter().any(|part| match part {
            hir::TplPart::Expr(expr) => expr_assigns_to(expr, name),
            hir::TplPart::Text(_) => false,
        }),
        K::Cond { cond, then, els } => {
            expr_assigns_to(cond, name) || expr_assigns_to(then, name) || expr_assigns_to(els, name)
        }
        K::Yield(value) => value
            .as_deref()
            .is_some_and(|value| expr_assigns_to(value, name)),
        K::Lambda { .. }
        | K::Int(_)
        | K::Float(_)
        | K::Bool(_)
        | K::Str(_)
        | K::Null
        | K::This
        | K::Local(_)
        | K::Global(_)
        | K::FuncRef(_)
        | K::EnumMember { .. }
        | K::Zero
        | K::RawNew { .. }
        | K::AsyncSuspend => false,
    }
}
