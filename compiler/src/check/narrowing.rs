//! Shared-location narrowing effects (compiler.md §124).

use super::expr::path_key;
use super::{Checker, FactSet, FnCtx, NarrowingFact};
use crate::check::rejection::RejectionSite;
use crate::hir::{BinOp, ExprKind};
use crate::types::Type;
use crate::{diag::Pos, hir};

impl Checker<'_> {
    pub(super) fn narrowing_paths(
        &self,
        cond: &hir::Expr,
        fx: &mut FnCtx,
    ) -> (Vec<NarrowingFact>, Vec<NarrowingFact>) {
        if let hir::ExprKind::Binary {
            op: op @ (hir::BinOp::And | hir::BinOp::Or),
            left,
            right,
        } = &cond.kind
        {
            // `a && b` is true, and `a || b` is false, when both operands
            // are; a kill in `b` ends a fact of `a` (compiler.md §124).
            let and = matches!(op, hir::BinOp::And);
            let pick = |facts: (Vec<NarrowingFact>, Vec<NarrowingFact>)| {
                if and {
                    facts.0
                } else {
                    facts.1
                }
            };
            let mut first = pick(self.narrowing_paths(left, fx));
            if !first.is_empty() {
                let effects =
                    right.narrowing_effects(self.narrowing_classes(), self.narrowing_helpers());
                first.retain(|key| {
                    let (shared, local) = path_kills(key, &effects);
                    if shared && !local {
                        fx.ended_shared_narrowing.insert_fact(key.clone());
                    }
                    !shared && !local
                });
            }
            first.extend(pick(self.narrowing_paths(right, fx)));
            if and {
                (first, Vec::new())
            } else {
                (Vec::new(), first)
            }
        } else {
            leaf_paths(cond, |ty| self.apparent_type(ty), self.narrowing_classes())
        }
    }

    pub(super) fn end_shared_narrowing(&self, expression: &hir::Expr, fx: &mut FnCtx) {
        if fx.has_narrowing_facts() {
            self.apply_narrowing_effects(
                &expression.operation_narrowing_effects(
                    self.narrowing_classes(),
                    self.narrowing_helpers(),
                ),
                fx,
            );
        }
        if let hir::ExprKind::Assign {
            op: None,
            target,
            value,
            ..
        } = &expression.kind
        {
            if matches!(
                self.apparent_type(&target.ty),
                crate::types::Type::Nullable(_)
            ) && !matches!(
                self.apparent_type(&value.ty),
                crate::types::Type::Null
                    | crate::types::Type::Nullable(_)
                    | crate::types::Type::Error
            ) {
                if let Some(key) = path_key(target) {
                    fx.narrowed.replace_fact(NarrowingFact {
                        key: key.clone(),
                        shared: target.is_shared_location(self.narrowing_classes()),
                    });
                    fx.ended_shared_narrowing.remove(&key);
                }
            }
        }
    }

    pub(super) fn nullable_use_error(
        &mut self,
        expression: &hir::Expr,
        fx: &FnCtx,
        sites: (RejectionSite, RejectionSite),
        message: String,
        pos: Pos,
    ) {
        let nullable_function = matches!(self.apparent_type(&expression.ty), crate::types::Type::Nullable(inner) if matches!(self.apparent_type(&inner), crate::types::Type::Func(_)));
        if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && path_key(expression).is_some_and(|key| fx.ended_shared_narrowing.contains(&key))
        {
            self.reject_subset(sites.1, message, pos);
        } else {
            self.reject_subset(sites.0, message, pos);
        }
    }
}

/// HIR effects available before flow checks reach a loop head (compiler.md §124).
#[derive(Default)]
pub(super) struct Analysis {
    classes: Vec<hir::ClassDef>,
    helpers: std::collections::HashSet<hir::Symbol>,
    loops: std::collections::HashMap<(String, u32, u32), hir::NarrowingEffects>,
}

fn position_key(pos: &Pos) -> (String, u32, u32) {
    (pos.file.clone(), pos.line, pos.col)
}

impl Analysis {
    pub(super) fn from_module(module: &mut hir::Module) -> Self {
        let classes = module.classes.clone();
        let mut analysis = Self {
            helpers: module.synthesized_helpers.clone(),
            ..Default::default()
        };
        for owner in module.expression_owners_mut() {
            match owner {
                hir::ExpressionOwnerMut::Expr(expression) => {
                    analysis.expression(expression, &classes)
                }
                hir::ExpressionOwnerMut::Body(body) => {
                    for statement in body {
                        analysis.statement(statement, &classes);
                    }
                }
            }
        }
        analysis.classes = classes;
        analysis
    }

    /// Adds the loop effects of `other` (compiler.md §135.1 rule 1).
    pub(super) fn merge_loops(&mut self, other: Self) {
        for (key, effects) in other.loops {
            self.loops.entry(key).or_default().merge(effects);
        }
    }

    fn function(&mut self, function: &hir::Function, classes: &[hir::ClassDef]) {
        for default in function
            .params
            .iter()
            .filter_map(|parameter| parameter.default.as_ref())
        {
            self.expression(default, classes);
        }
        for statement in &function.body {
            self.statement(statement, classes);
        }
    }

    fn expression(&mut self, expression: &hir::Expr, classes: &[hir::ClassDef]) {
        for child in expression.children() {
            self.child(child, classes);
        }
    }

    fn statement(&mut self, statement: &hir::Stmt, classes: &[hir::ClassDef]) {
        if let hir::Stmt::While { pos, .. }
        | hir::Stmt::For { pos, .. }
        | hir::Stmt::ForOf { pos, .. } = statement
        {
            self.loops
                .entry(position_key(pos))
                .or_default()
                .merge(statement.narrowing_effects(classes, &self.helpers));
        }
        for child in statement.children() {
            self.child(child, classes);
        }
    }

    fn child(&mut self, child: hir::HirChild<'_>, classes: &[hir::ClassDef]) {
        match child {
            hir::HirChild::Expr(expression) => self.expression(expression, classes),
            hir::HirChild::Stmt(statement) => self.statement(statement, classes),
        }
    }
}

impl Checker<'_> {
    /// The loop effects of the bodies that the opaque check added after
    /// `functions` functions, `classes` classes, and `methods[i]` methods
    /// of each earlier class (compiler.md §135.1 rule 1).
    pub(super) fn opaque_body_loop_effects(
        &self,
        functions: usize,
        classes: usize,
        methods: &[usize],
    ) -> Analysis {
        let mut analysis = Analysis {
            helpers: self.narrowing_helpers().clone(),
            ..Default::default()
        };
        for function in self.functions.iter().skip(functions) {
            analysis.function(function, &self.classes);
        }
        for (index, class) in self.classes.iter().enumerate() {
            if index < classes {
                let first = methods.get(index).copied().unwrap_or(0);
                for method in class.methods.iter().skip(first) {
                    analysis.function(method, &self.classes);
                }
                continue;
            }
            for init in class.fields.iter().filter_map(|field| field.init.as_ref()) {
                analysis.expression(init, &self.classes);
            }
            for function in class.ctor.iter().chain(class.methods.iter()) {
                analysis.function(function, &self.classes);
            }
        }
        analysis
    }

    fn narrowing_helpers(&self) -> &std::collections::HashSet<hir::Symbol> {
        self.narrowing_analysis
            .as_ref()
            .map_or(&self.narrowing_helper_symbols, |analysis| &analysis.helpers)
    }

    fn narrowing_classes(&self) -> &[hir::ClassDef] {
        self.narrowing_analysis
            .as_ref()
            .map_or(&self.classes, |analysis| &analysis.classes)
    }

    pub(super) fn apply_narrowing_effects(&self, effects: &hir::NarrowingEffects, fx: &mut FnCtx) {
        if !fx.has_narrowing_facts() {
            return;
        }
        fx.ended_shared_narrowing
            .retain(|key| !path_kills(key, effects).1);
        for index in fx.shadowed_narrowing_scopes.iter() {
            let scope = &mut fx.scopes[*index];
            scope.shadowed_narrowing.retain(|key| {
                // A shadowed path belongs to another binding, even with the same spelling.
                if path_kills(key, effects).0 {
                    scope.shadowed_ended_shared.insert_fact(key.clone());
                    false
                } else {
                    true
                }
            });
        }
        fx.narrowed.retain(|key| {
            let (shared_kill, local_kill) = path_kills(key, effects);
            if shared_kill {
                fx.ended_shared_narrowing.insert_fact(key.clone());
            }
            if local_kill {
                fx.ended_shared_narrowing.remove(key);
            }
            !shared_kill && !local_kill
        });
    }

    pub(super) fn end_loop_narrowing(&self, pos: &Pos, fx: &mut FnCtx) {
        if !fx.has_narrowing_facts() {
            return;
        }
        if let Some(effects) = self
            .narrowing_analysis
            .as_ref()
            .and_then(|analysis| analysis.loops.get(&position_key(pos)))
        {
            self.apply_narrowing_effects(effects, fx);
        }
    }

    pub(super) fn end_scope_narrowing(&self, fx: &mut FnCtx) {
        if !fx.has_narrowing_facts() {
            return;
        }
        if fx.scopes.last().is_some_and(|scope| scope.dispose_on_exit) {
            self.apply_narrowing_effects(
                &hir::NarrowingEffects {
                    script: true,
                    ..Default::default()
                },
                fx,
            );
        }
    }
}

impl Checker<'_> {
    pub(super) fn body_narrowing_effects(
        &self,
        body: &[hir::Stmt],
        fx: &FnCtx,
        incoming: &super::NarrowingFacts,
        notes: &super::NarrowingFacts,
    ) -> Option<hir::NarrowingEffects> {
        (fx.has_narrowing_facts() || !incoming.is_empty() || !notes.is_empty()).then(|| {
            hir::NarrowingEffects::body(body, self.narrowing_classes(), self.narrowing_helpers())
        })
    }
}

impl FnCtx {
    fn has_narrowing_facts(&self) -> bool {
        !self.narrowed.is_empty()
            || !self.ended_shared_narrowing.is_empty()
            || !self.shadowed_narrowing_scopes.is_empty()
    }

    pub(super) fn narrowing_note_paths(&self) -> std::collections::HashSet<NarrowingFact> {
        self.narrowed
            .union(&self.ended_shared_narrowing)
            .cloned()
            .collect()
    }

    pub(super) fn finish_narrowing_join(
        &mut self,
        eligible: &std::collections::HashSet<NarrowingFact>,
    ) {
        self.ended_shared_narrowing
            .retain(|key| eligible.contains(key) && !self.narrowed.contains(key));
    }
}

fn path_kills(key: &NarrowingFact, effects: &hir::NarrowingEffects) -> (bool, bool) {
    let root = super::stmt::root_of(key);
    let global = root.starts_with("[[global]]");
    let shared = key.shared;
    let shared_kill = shared
        && (effects.script
            || key
                .split('.')
                .skip(1)
                .any(|field| effects.fields.contains(field)))
        || effects.globals.iter().any(|name| {
            let stored = format!("[[global]]{}", name.full_text());
            key.key == *stored || key.starts_with(&format!("{stored}."))
        });
    let same_path = |stored: &String| key.key == *stored || key.starts_with(&format!("{stored}."));
    let local_kill =
        effects.nullable_store_ends(key) || !global && effects.locals.iter().any(same_path);
    (shared_kill, local_kill)
}

/// Narrowing facts derived from a checked leaf condition: paths known
/// non-null or known present when the condition is true / false.
/// `Checker::narrowing_paths` splits `&&` and `||` and applies the kills
/// of compiler.md §124 before it reaches a leaf.
pub(super) fn leaf_paths(
    cond: &hir::Expr,
    apparent_type: impl Fn(&Type) -> Type,
    classes: &[hir::ClassDef],
) -> (Vec<NarrowingFact>, Vec<NarrowingFact>) {
    let fact = |value: &hir::Expr| {
        path_key(value).map(|key| NarrowingFact {
            key,
            shared: value.is_shared_location(classes),
        })
    };
    if let Some(value) = super::exception::instanceof_narrowed_value(cond) {
        return (fact(value).into_iter().collect(), Vec::new());
    }
    if let ExprKind::AbsenceTest { value, negated } = &cond.kind {
        if let Some(key) = fact(value) {
            return if *negated {
                (vec![key], Vec::new())
            } else {
                (Vec::new(), vec![key])
            };
        }
        return (Vec::new(), Vec::new());
    }
    if let ExprKind::Binary { op, left, right } = &cond.kind {
        match op {
            BinOp::Eq | BinOp::Ne => {
                let (null_side, other) = if matches!(left.kind, ExprKind::Null) {
                    (Some(()), right)
                } else if matches!(right.kind, ExprKind::Null) {
                    (Some(()), left)
                } else {
                    (None, left)
                };
                if null_side.is_some() && matches!(apparent_type(&other.ty), Type::Nullable(_)) {
                    let target = match &other.kind {
                        ExprKind::Assign {
                            op: None, target, ..
                        } => target.as_ref(),
                        _ => other,
                    };
                    if let Some(key) = fact(target) {
                        return match op {
                            // `p === null` → p is non-null when false.
                            BinOp::Eq => (Vec::new(), vec![key]),
                            // `p !== null` → p is non-null when true.
                            _ => (vec![key], Vec::new()),
                        };
                    }
                }
                (Vec::new(), Vec::new())
            }
            _ => (Vec::new(), Vec::new()),
        }
    } else {
        (Vec::new(), Vec::new())
    }
}

impl Checker<'_> {
    /// Records each synthesized helper once, when the checker creates it.
    pub(super) fn push_narrowing_helper(&mut self, helper: hir::Function) {
        self.narrowing_helper_symbols.insert(helper.symbol.clone());
        self.functions.push(helper);
    }
}
