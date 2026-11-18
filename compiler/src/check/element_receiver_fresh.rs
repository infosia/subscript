//! Fresh local reads for the move reach (compiler.md §333 rules 1 to 3).

use std::collections::{HashMap, HashSet};

use crate::hir::{Expr, ExprKind, HirChild, Param, Stmt};

type Env<'a> = HashMap<&'a str, usize>;

#[derive(Default)]
struct Locals<'a> {
    fresh: Vec<bool>,
    reads: Vec<(&'a Expr, usize)>,
}

impl<'a> Locals<'a> {
    fn bind(&mut self, env: &mut Env<'a>, name: &'a str, fresh: bool) {
        env.insert(name, self.fresh.len());
        self.fresh.push(fresh);
    }

    fn sequence(&mut self, body: &'a [Stmt], env: &mut Env<'a>) {
        for stmt in body {
            self.stmt(stmt, env);
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt, env: &mut Env<'a>) {
        match stmt {
            Stmt::Let { name, init, .. } => {
                self.expr(init, env);
                // An unassigned declaration gives the local no value (§333 rule 1).
                self.bind(
                    env,
                    name,
                    literal(init) || matches!(init.kind, ExprKind::Unassigned),
                );
            }
            Stmt::Block(body) => self.sequence(body, &mut env.clone()),
            Stmt::If {
                cond, then, els, ..
            } => {
                self.expr(cond, env);
                self.sequence(then, &mut env.clone());
                if let Some(els) = els {
                    self.sequence(els, &mut env.clone());
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond, env);
                self.sequence(body, &mut env.clone());
            }
            Stmt::DoWhile { body, cond, .. } => {
                self.sequence(body, &mut env.clone());
                self.expr(cond, env);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut env = env.clone();
                if let Some(init) = init {
                    self.stmt(init, &mut env);
                }
                if let Some(cond) = cond {
                    self.expr(cond, &env);
                }
                self.sequence(body, &mut env.clone());
                self.sequence(step, &mut env);
            }
            Stmt::ForOf {
                name,
                subject,
                body,
                ..
            }
            | Stmt::GeneratorForOf {
                name,
                subject,
                body,
                ..
            } => {
                self.expr(subject, env);
                let mut env = env.clone();
                self.bind(&mut env, name, false);
                self.sequence(body, &mut env);
            }
            Stmt::Switch { disc, cases, .. } => {
                self.expr(disc, env);
                let mut env = env.clone();
                for case in cases {
                    if let Some(test) = &case.test {
                        self.expr(test, &env);
                    }
                    self.sequence(&case.body, &mut env);
                }
            }
            Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                self.sequence(body, &mut env.clone());
                let mut env = env.clone();
                if let Some((name, _)) = binding {
                    self.bind(&mut env, name, false);
                }
                self.sequence(handler, &mut env);
            }
            Stmt::Using {
                bindings,
                body,
                finalizer,
                ..
            } => {
                let mut env = env.clone();
                for binding in bindings {
                    self.bind(&mut env, &binding.name, false);
                }
                self.sequence(body, &mut env.clone());
                if let Some(finalizer) = finalizer {
                    self.sequence(finalizer, &mut env);
                }
            }
            Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Throw { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_) => {
                for child in stmt.children() {
                    self.child(child, env);
                }
            }
        }
    }

    fn child(&mut self, child: HirChild<'a>, env: &Env<'a>) {
        match child {
            HirChild::Expr(expr) => self.expr(expr, env),
            HirChild::Stmt(stmt) => self.stmt(stmt, &mut env.clone()),
        }
    }

    fn expr(&mut self, expr: &'a Expr, env: &Env<'a>) {
        match &expr.kind {
            // A captured read belongs to another body (compiler.md §333 rule 2).
            ExprKind::Lambda { .. } => return,
            ExprKind::Local(name, ..) => {
                if let Some(&id) = env.get(name.as_str()) {
                    self.reads.push((expr, id));
                }
            }
            ExprKind::Assign { target, value, .. } => {
                if let ExprKind::Local(name, ..) = &target.kind {
                    if let Some(&id) = env.get(name.as_str()) {
                        self.fresh[id] &= literal(value);
                    }
                }
            }
            _ => {}
        }
        for child in expr.children() {
            self.child(child, env);
        }
    }
}

fn literal(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::ArrayLit(_) | ExprKind::ArraySpreadLit(_)
    )
}

/// Each result points to a read whose binding receives only array literals.
/// Generator locals, parameters, and captures cannot qualify (compiler.md §333 rules 1 and 3).
pub(super) fn reads(params: &[Param], body: &[Stmt], is_generator: bool) -> HashSet<*const Expr> {
    if is_generator {
        return HashSet::new();
    }
    let mut locals = Locals::default();
    let mut env = Env::new();
    for param in params {
        locals.bind(&mut env, &param.name, false);
    }
    locals.sequence(body, &mut env);
    locals
        .reads
        .into_iter()
        .filter_map(|(expr, id)| locals.fresh[id].then_some(expr as *const Expr))
        .collect()
}
