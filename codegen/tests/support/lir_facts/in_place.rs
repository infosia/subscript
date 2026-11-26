//! The second derivation of the in-place `filter` calls (`compiler.md`
//! §359 rule 13): rules 2 to 4, read from HIR alone.
//!
//! This derivation shares two things with the decision in
//! `codegen/src/lir/in_place_filter.rs`: the walk over `children()` of
//! each HIR node, and the loop model (each loop statement and each lambda
//! body is a loop). If that model is wrong, both are wrong in the same
//! way. Rule 14 is the check that shares neither: the test
//! `codegen/tests/in_place_filter.rs`
//! `each_entry_with_an_in_place_filter_prints_the_output_of_a_filter_that_allocates`
//! runs each such corpus entry with the decision off and compares outputs.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// A static `filter` call: its position and the position of its callback
/// argument. The calls of one chain share the position of the chain start.
pub(super) type FilterSite = (String, u32, u32, String, u32, u32);

pub(super) fn filter_site(call: &hir::Expr, args: &[hir::Expr]) -> Option<FilterSite> {
    let callback = args.get(1)?;
    Some((
        call.pos.file.clone(),
        call.pos.line,
        call.pos.col,
        callback.pos.file.clone(),
        callback.pos.line,
        callback.pos.col,
    ))
}

/// §8.1d B: a dynamic array receiver and a named function or a lambda
/// literal, also inside a conversion.
fn lowers_inline(args: &[hir::Expr]) -> bool {
    let Some(Type::Array(_)) = args.first().map(|receiver| &receiver.ty) else {
        return false;
    };
    let Some(mut callback) = args.get(1) else {
        return false;
    };
    if let hir::ExprKind::Cast(inner) = &callback.kind {
        callback = inner;
    }
    matches!(
        callback.kind,
        hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }
    )
}

/// Rule 2a.
fn makes_new_array(expr: &hir::Expr) -> bool {
    if let hir::ExprKind::ArrayLit(_) = expr.kind {
        return matches!(expr.ty, Type::Array(_));
    }
    let hir::ExprKind::Call {
        callee: hir::Callee::Arr(operation),
        args,
    } = &expr.kind
    else {
        return false;
    };
    matches!(operation, hir::ArrFn::Map | hir::ArrFn::Filter) && lowers_inline(args)
}

/// The loops around a node, by the position of each loop statement or
/// lambda. A lambda body can run more than once, so it is a loop here.
type Loops = Vec<(String, u32, u32)>;

#[derive(Default)]
struct BodyFacts {
    names: BTreeMap<String, usize>,
    uses: BTreeMap<String, usize>,
    candidates: BTreeMap<String, Loops>,
    calls: Vec<(FilterSite, Option<String>, bool, Loops, bool)>,
}

struct Collector<'f> {
    facts: &'f mut BodyFacts,
    loops: Loops,
    async_depth: u32,
}

impl Collector<'_> {
    fn enter(&mut self, pos: &Pos) {
        self.loops.push((pos.file.clone(), pos.line, pos.col));
    }

    fn leave(&mut self) {
        self.loops.pop();
    }

    fn name(&mut self, name: &str) {
        *self.facts.names.entry(name.to_owned()).or_insert(0) += 1;
    }

    fn block(&mut self, statements: &[hir::Stmt]) {
        statements.iter().for_each(|statement| self.stmt(statement));
    }

    fn stmt(&mut self, statement: &hir::Stmt) {
        use hir::Stmt as S;
        match statement {
            S::Let {
                name,
                ty,
                mutable,
                init,
                ..
            } => {
                self.expr(init);
                self.name(name);
                if !mutable && init.ty == *ty && makes_new_array(init) {
                    self.facts
                        .candidates
                        .insert(name.clone(), self.loops.clone());
                }
            }
            S::While { cond, body, pos } | S::DoWhile { cond, body, pos } => {
                self.enter(pos);
                self.expr(cond);
                self.block(body);
                self.leave();
            }
            S::For {
                init,
                cond,
                step,
                body,
                pos,
            } => {
                if let Some(init) = init.as_deref() {
                    self.stmt(init);
                }
                self.enter(pos);
                cond.iter().for_each(|cond| self.expr(cond));
                self.block(step);
                self.block(body);
                self.leave();
            }
            S::ForOf {
                name,
                subject,
                body,
                pos,
                ..
            }
            | S::GeneratorForOf {
                name,
                subject,
                body,
                pos,
                ..
            } => {
                self.expr(subject);
                self.enter(pos);
                self.name(name);
                self.block(body);
                self.leave();
            }
            S::Try {
                body,
                binding,
                handler,
                ..
            } => {
                self.block(body);
                if let Some((name, _)) = binding {
                    self.name(name);
                }
                self.block(handler);
            }
            S::Using { bindings, .. } => {
                bindings.iter().for_each(|binding| self.name(&binding.name));
                self.children(statement.children());
            }
            _ => self.children(statement.children()),
        }
    }

    fn children(&mut self, children: Vec<hir::HirChild<'_>>) {
        for child in children {
            match child {
                hir::HirChild::Expr(expr) => self.expr(expr),
                hir::HirChild::Stmt(statement) => self.stmt(statement),
            }
        }
    }

    fn expr(&mut self, expr: &hir::Expr) {
        match &expr.kind {
            hir::ExprKind::Local(name, _, _) => {
                *self.facts.uses.entry(name.clone()).or_insert(0) += 1;
            }
            hir::ExprKind::Lambda {
                params,
                captures,
                is_async,
                ..
            } => {
                for capture in captures {
                    *self.facts.uses.entry(capture.name.clone()).or_insert(0) += 1;
                }
                self.async_depth += u32::from(*is_async);
                self.enter(&expr.pos);
                params.iter().for_each(|param| self.name(&param.name));
                self.children(expr.children());
                self.leave();
                self.async_depth -= u32::from(*is_async);
            }
            hir::ExprKind::Call {
                callee: hir::Callee::Arr(hir::ArrFn::Filter),
                args,
            } if lowers_inline(args) => {
                if let Some(site) = filter_site(expr, args) {
                    let receiver = &args[0];
                    let local = match &receiver.kind {
                        hir::ExprKind::Local(name, _, _) => Some(name.clone()),
                        _ => None,
                    };
                    self.facts.calls.push((
                        site,
                        local,
                        makes_new_array(receiver),
                        self.loops.clone(),
                        self.async_depth > 0,
                    ));
                }
                self.children(expr.children());
            }
            _ => self.children(expr.children()),
        }
    }
}

/// True if `call` is a static `filter` call in `in_place`, the result of
/// [`in_place_sites`] for its module.
pub(super) fn is_in_place(in_place: &BTreeSet<FilterSite>, call: &hir::Expr) -> bool {
    let hir::ExprKind::Call {
        callee: hir::Callee::Arr(hir::ArrFn::Filter),
        args,
    } = &call.kind
    else {
        return false;
    };
    lowers_inline(args) && filter_site(call, args).is_some_and(|site| in_place.contains(&site))
}

/// The static `filter` calls of `hir` that rules 2 to 4 put in place.
pub(super) fn in_place_sites(hir: &hir::Module) -> BTreeSet<FilterSite> {
    let mut sites = BTreeSet::new();
    for owner in hir.expression_owners() {
        let hir::ExpressionOwner::Body {
            statements,
            function,
        } = owner
        else {
            continue;
        };
        let mut facts = BodyFacts::default();
        let mut collector = Collector {
            facts: &mut facts,
            loops: Vec::new(),
            async_depth: 0,
        };
        if let Some(function) = function {
            function
                .params
                .iter()
                .for_each(|param| collector.name(&param.name));
        }
        collector.block(statements);
        // Rule 4: a generator or an async function has no in-place form.
        if function.is_some_and(|function| function.is_generator || function.is_async) {
            continue;
        }
        for (site, local, new_receiver, loops, in_async) in &facts.calls {
            if *in_async {
                continue;
            }
            let one_reader = local.as_ref().is_some_and(|name| {
                facts.names.get(name).copied() == Some(1)
                    && facts.uses.get(name).copied() == Some(1)
                    // Rule 3: the loops of the call are those of the declaration.
                    && facts.candidates.get(name) == Some(loops)
            });
            if *new_receiver || one_reader {
                sites.insert(site.clone());
            }
        }
    }
    sites
}
