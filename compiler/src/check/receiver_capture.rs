//! Receiver-capture uses in the constructor assignment prefix (§157).

use super::{hir, PrefixThis};
use crate::diag::Pos;
use std::collections::HashMap;

/// A capture named `this` is the immutable lexical receiver.
/// Other captures carry this fact through local lambda bindings.
pub(super) fn carries_receiver(expression: &hir::Expr, locals: &HashMap<String, bool>) -> bool {
    match &expression.kind {
        hir::ExprKind::Local(name, _, _) => locals.get(name).copied().unwrap_or(false),
        hir::ExprKind::Lambda { captures, .. } => captures.iter().any(|capture| {
            capture.name == "this" || locals.get(&capture.name).copied().unwrap_or(false)
        }),
        hir::ExprKind::Cast(value) | hir::ExprKind::Assign { value, .. } => {
            carries_receiver(value, locals)
        }
        hir::ExprKind::Cond { then, els, .. } => {
            carries_receiver(then, locals) || carries_receiver(els, locals)
        }
        _ => false,
    }
}

/// Collect calls that can use a lambda's receiver before all fields hold values.
/// A lambda declaration does not execute its body.
pub(super) fn collect(
    node: hir::HirChild<'_>,
    locals: &mut HashMap<String, bool>,
    out: &mut Vec<(Pos, PrefixThis)>,
) {
    walk(node, locals, out, &mut HashMap::new());
}

fn walk(
    node: hir::HirChild<'_>,
    locals: &mut HashMap<String, bool>,
    out: &mut Vec<(Pos, PrefixThis)>,
    exceptional: &mut HashMap<String, bool>,
) {
    match node {
        hir::HirChild::Stmt(statement) => match statement {
            hir::Stmt::Let { name, init, .. } => {
                walk(hir::HirChild::Expr(init), locals, out, exceptional);
                locals.insert(name.clone(), carries_receiver(init, locals));
            }
            hir::Stmt::Block(body) | hir::Stmt::Using { body, .. } => {
                scope(body, locals, out, exceptional);
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                walk(hir::HirChild::Expr(cond), locals, out, exceptional);
                let mut left = locals.clone();
                let mut right = locals.clone();
                scope(then, &mut left, out, exceptional);
                if let Some(body) = els {
                    scope(body, &mut right, out, exceptional);
                }
                merge(&mut left, &right);
                *locals = left;
            }
            hir::Stmt::While { cond, body, .. } => {
                loop_facts(locals, out, |facts, out| {
                    walk(hir::HirChild::Expr(cond), facts, out, exceptional);
                    scope(body, facts, out, exceptional);
                });
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut loop_scope = locals.clone();
                if let Some(statement) = init {
                    walk(
                        hir::HirChild::Stmt(statement),
                        &mut loop_scope,
                        out,
                        exceptional,
                    );
                }
                loop_facts(&mut loop_scope, out, |facts, out| {
                    if let Some(value) = cond {
                        walk(hir::HirChild::Expr(value), facts, out, exceptional);
                    }
                    scope(body, facts, out, exceptional);
                    if let Some(value) = step {
                        walk(hir::HirChild::Expr(value), facts, out, exceptional);
                    }
                });
                for (name, fact) in locals.iter_mut() {
                    if !matches!(init.as_deref(), Some(hir::Stmt::Let { name: binding, .. }) if binding == name)
                    {
                        *fact |= loop_scope.get(name).copied().unwrap_or(false);
                    }
                }
            }
            hir::Stmt::ForOf {
                name,
                subject,
                body,
                ..
            } => {
                walk(hir::HirChild::Expr(subject), locals, out, exceptional);
                let mut loop_scope = locals.clone();
                loop_scope.insert(name.clone(), false);
                loop_facts(&mut loop_scope, out, |facts, out| {
                    scope(body, facts, out, exceptional)
                });
                for (binding, fact) in locals.iter_mut() {
                    if binding != name {
                        *fact |= loop_scope.get(binding).copied().unwrap_or(false);
                    }
                }
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                walk(hir::HirChild::Expr(disc), locals, out, exceptional);
                let entry = locals.clone();
                let mut exits = entry.keys().map(|name| (name.clone(), false)).collect();
                let mut fallthrough = entry.clone();
                for case in cases {
                    if let Some(value) = &case.test {
                        walk(hir::HirChild::Expr(value), locals, out, exceptional);
                    }
                    merge(&mut fallthrough, &entry);
                    scope(&case.body, &mut fallthrough, out, exceptional);
                    merge(&mut exits, &fallthrough);
                    if case
                        .body
                        .iter()
                        .any(|statement| matches!(statement, hir::Stmt::Break(_)))
                    {
                        fallthrough = entry.clone();
                    }
                }
                if cases.iter().all(|case| case.test.is_some()) {
                    merge(&mut exits, &entry);
                }
                *locals = exits;
            }
            hir::Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                let mut normal = locals.clone();
                let mut raised = locals.keys().map(|name| (name.clone(), false)).collect();
                scope(body, &mut normal, out, &mut raised);
                // The handler consumes these edges. Only its own throws reach an outer handler.
                let mut catch_scope = raised;
                if let Some((name, _)) = binding {
                    catch_scope.insert(name.clone(), false);
                }
                scope(handler, &mut catch_scope, out, exceptional);
                for (name, fact) in locals.iter_mut() {
                    *fact = normal.get(name).copied().unwrap_or(false);
                    if binding.as_ref().is_none_or(|(binding, _)| binding != name) {
                        *fact |= catch_scope.get(name).copied().unwrap_or(false);
                    }
                }
            }
            hir::Stmt::Throw { value, .. } => {
                walk(hir::HirChild::Expr(value), locals, out, exceptional);
                merge(exceptional, locals);
            }

            _ => {
                for child in statement.children() {
                    walk(child, locals, out, exceptional);
                }
            }
        },
        hir::HirChild::Expr(expression) => {
            if matches!(expression.kind, hir::ExprKind::Lambda { .. }) {
                return;
            }
            if let hir::ExprKind::Cond { cond, then, els } = &expression.kind {
                walk(hir::HirChild::Expr(cond), locals, out, exceptional);
                let mut left = locals.clone();
                let mut right = locals.clone();
                walk(hir::HirChild::Expr(then), &mut left, out, exceptional);
                walk(hir::HirChild::Expr(els), &mut right, out, exceptional);
                merge(&mut left, &right);
                *locals = left;
                return;
            }
            if let hir::ExprKind::Binary {
                op: hir::BinOp::And | hir::BinOp::Or,
                left,
                right,
            } = &expression.kind
            {
                walk(hir::HirChild::Expr(left), locals, out, exceptional);
                let mut evaluated = locals.clone();
                walk(hir::HirChild::Expr(right), &mut evaluated, out, exceptional);
                merge(locals, &evaluated);
                return;
            }
            let call = matches!(
                expression.kind,
                hir::ExprKind::Call { .. }
                    | hir::ExprKind::New { .. }
                    | hir::ExprKind::AsyncCall { .. }
                    | hir::ExprKind::AsyncHandleCreate { .. }
            );
            let mut receiver_use = false;
            for child in expression.children() {
                walk(child, locals, out, exceptional);
                if let hir::HirChild::Expr(value) = child {
                    receiver_use |= carries_receiver(value, locals);
                }
            }
            if call {
                merge(exceptional, locals);
            }
            if call
                && receiver_use
                && !out
                    .iter()
                    .any(|(pos, kind)| *pos == expression.pos && matches!(kind, PrefixThis::Call))
            {
                out.push((expression.pos.clone(), PrefixThis::Call));
            }
            if let hir::ExprKind::Assign { target, value, .. } = &expression.kind {
                if let hir::ExprKind::Local(name, _, _) = &target.kind {
                    locals.insert(name.clone(), carries_receiver(value, locals));
                }
            }
        }
    }
}

/// A block's declarations shadow outer facts; its assignments can update them.
fn scope(
    body: &[hir::Stmt],
    locals: &mut HashMap<String, bool>,
    out: &mut Vec<(Pos, PrefixThis)>,
    exceptional: &mut HashMap<String, bool>,
) {
    let shadows = |name: &String| {
        body.iter().any(|statement| matches!(statement, hir::Stmt::Let { name: binding, .. } if binding == name))
    };
    let mut inner = locals.clone();
    let mut raised = exceptional.clone();
    raised.retain(|name, _| !shadows(name));
    for statement in body {
        walk(hir::HirChild::Stmt(statement), &mut inner, out, &mut raised);
    }
    merge(exceptional, &raised);
    for (name, fact) in locals.iter_mut() {
        if !shadows(name) {
            *fact = inner.get(name).copied().unwrap_or(false);
        }
    }
}

fn merge(locals: &mut HashMap<String, bool>, other: &HashMap<String, bool>) {
    for (name, fact) in locals {
        *fact |= other.get(name).copied().unwrap_or(false);
    }
}

/// Join loop entries until no receiver fact changes.
fn loop_facts(
    locals: &mut HashMap<String, bool>,
    out: &mut Vec<(Pos, PrefixThis)>,
    mut body: impl FnMut(&mut HashMap<String, bool>, &mut Vec<(Pos, PrefixThis)>),
) {
    loop {
        let mut iteration = locals.clone();
        body(&mut iteration, out);
        let previous = locals.clone();
        merge(locals, &iteration);
        if *locals == previous {
            break;
        }
    }
}
