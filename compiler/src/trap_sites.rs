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
            _ => {
                for child in stmt.children_mut() {
                    match child {
                        hir::HirChildMut::Expr(expr) => self.expr(expr),
                        hir::HirChildMut::Stmt(stmt) => self.stmt(stmt),
                    }
                }
            }
        }
    }

    fn expr(&mut self, expr: &mut hir::Expr) {
        if let K::Lambda { body, .. } = &mut expr.kind {
            Analyzer::default().stmts(body);
            return;
        }
        for child in expr.children_mut() {
            match child {
                hir::HirChildMut::Expr(child) => self.expr(child),
                hir::HirChildMut::Stmt(statement) => self.stmt(statement),
            }
        }
        if let K::Index {
            obj,
            index,
            checked,
        } = &mut expr.kind
        {
            *checked = match &obj.ty {
                Type::FixedArray(_, len) => !self.index_in_bounds(index, *len),
                // Dynamic arrays have no compile-time length proof.
                _ => true,
            };
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
    stmt.children().into_iter().any(|child| match child {
        hir::HirChild::Expr(expr) => expr_assigns_to(expr, name),
        hir::HirChild::Stmt(stmt) => stmt_assigns_to(stmt, name),
    })
}

fn expr_assigns_to(expr: &hir::Expr, name: &str) -> bool {
    if matches!(expr.kind, K::Lambda { .. }) {
        return false;
    }
    matches!(&expr.kind, K::Assign { target, .. }
        if matches!(&target.kind, K::Local(local) if local == name))
        || expr.children().into_iter().any(|child| match child {
            hir::HirChild::Expr(expr) => expr_assigns_to(expr, name),
            hir::HirChild::Stmt(stmt) => stmt_assigns_to(stmt, name),
        })
}
