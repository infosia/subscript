//! Shared-location narrowing effects (compiler.md §124).

use super::expr::path_key;
use super::{Checker, FnCtx};
use crate::{
    diag::{Pos, RuleCode},
    divergence::Divergence,
    hir,
};

impl Checker<'_> {
    pub(super) fn narrowing_paths(
        &self,
        cond: &hir::Expr,
        fx: &mut FnCtx,
    ) -> (Vec<String>, Vec<String>) {
        register_shared_paths(cond, self.narrowing_classes(), fx);
        if let hir::ExprKind::Binary {
            op: hir::BinOp::And,
            left,
            right,
        } = &cond.kind
        {
            let (mut first, _) = self.narrowing_paths(left, fx);
            let effects =
                right.narrowing_effects(self.narrowing_classes(), &self.narrowing_helpers());
            let shared_paths = fx.shared_narrowing_paths();
            first.retain(|key| {
                let (shared, local) = path_kills(key, &effects, &shared_paths);
                if shared && !local {
                    fx.ended_shared_narrowing.insert(key.clone());
                }
                !shared && !local
            });
            first.extend(self.narrowing_paths(right, fx).0);
            (first, Vec::new())
        } else {
            super::stmt::narrow_paths(cond)
        }
    }

    pub(super) fn end_shared_narrowing(&self, expression: &hir::Expr, fx: &mut FnCtx) {
        self.apply_narrowing_effects(
            &expression
                .operation_narrowing_effects(self.narrowing_classes(), &self.narrowing_helpers()),
            fx,
        );
    }

    pub(super) fn nullable_use_error(
        &mut self,
        expression: &hir::Expr,
        fx: &FnCtx,
        code: RuleCode,
        message: String,
        pos: Pos,
    ) {
        if path_key(expression).is_some_and(|key| fx.ended_shared_narrowing.contains(&key)) {
            self.error_diverging(code, message, pos, Divergence::SharedLocationNarrowing);
        } else {
            self.error(code, message, pos);
        }
    }
}

/// HIR effects available before flow checks reach a loop head (compiler.md §124).
#[derive(Default)]
pub(super) struct Analysis {
    classes: Vec<hir::ClassDef>,
    helpers: std::collections::HashSet<String>,
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
    fn narrowing_helpers(&self) -> std::collections::HashSet<String> {
        self.narrowing_analysis.as_ref().map_or_else(
            || {
                self.functions
                    .iter()
                    .filter(|function| function.synthesized_helper)
                    .map(|function| function.name.clone())
                    .collect()
            },
            |analysis| analysis.helpers.clone(),
        )
    }

    fn narrowing_classes(&self) -> &[hir::ClassDef] {
        self.narrowing_analysis
            .as_ref()
            .map_or(&self.classes, |analysis| &analysis.classes)
    }

    pub(super) fn apply_narrowing_effects(&self, effects: &hir::NarrowingEffects, fx: &mut FnCtx) {
        let shared_paths = fx.shared_narrowing_paths();
        fx.narrowed.retain(|key| {
            let (shared_kill, local_kill) = path_kills(key, effects, &shared_paths);
            if shared_kill {
                fx.ended_shared_narrowing.insert(key.clone());
            }
            if local_kill {
                fx.ended_shared_narrowing.remove(key);
            }
            !shared_kill && !local_kill
        });
    }

    pub(super) fn end_loop_narrowing(&self, pos: &Pos, fx: &mut FnCtx) {
        if let Some(effects) = self
            .narrowing_analysis
            .as_ref()
            .and_then(|analysis| analysis.loops.get(&position_key(pos)))
        {
            self.apply_narrowing_effects(effects, fx);
        }
    }

    pub(super) fn end_scope_narrowing(&self, body: &[hir::Stmt], fx: &mut FnCtx) {
        if body
            .iter()
            .any(|statement| matches!(statement, hir::Stmt::Let { dispose: true, .. }))
        {
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
    pub(super) fn body_narrowing_effects(&self, body: &[hir::Stmt]) -> hir::NarrowingEffects {
        hir::NarrowingEffects::body(body, self.narrowing_classes(), &self.narrowing_helpers())
    }
}

impl FnCtx {
    fn shared_narrowing_paths(&self) -> std::collections::HashSet<String> {
        let mut paths = std::collections::HashMap::new();
        for scope in &self.scopes {
            paths.extend(
                scope
                    .shared_narrowing_paths
                    .iter()
                    .map(|(key, shared)| (key.clone(), *shared)),
            );
        }
        paths
            .into_iter()
            .filter_map(|(key, shared)| shared.then_some(key))
            .collect()
    }

    pub(super) fn narrowing_note_paths(&self) -> std::collections::HashSet<String> {
        self.narrowed
            .union(&self.ended_shared_narrowing)
            .cloned()
            .collect()
    }

    pub(super) fn finish_narrowing_join(&mut self, eligible: &std::collections::HashSet<String>) {
        self.ended_shared_narrowing
            .retain(|key| eligible.contains(key) && !self.narrowed.contains(key));
    }
}

fn path_kills(
    key: &str,
    effects: &hir::NarrowingEffects,
    shared_paths: &std::collections::HashSet<String>,
) -> (bool, bool) {
    let root = super::stmt::root_of(key);
    let global = root.starts_with("[[global]]");
    let shared = shared_paths.contains(key);
    let shared_kill = shared
        && (effects.script
            || key
                .split('.')
                .skip(1)
                .any(|field| effects.fields.contains(field)))
        || effects.globals.iter().any(|name| {
            let stored = format!("[[global]]{name}");
            key == stored || key.starts_with(&format!("{stored}."))
        });
    let local_kill = !global
        && effects
            .locals
            .iter()
            .any(|stored| key == stored || key.starts_with(&format!("{stored}.")));
    (shared_kill, local_kill)
}

fn register_shared_paths(expression: &hir::Expr, classes: &[hir::ClassDef], fx: &mut FnCtx) {
    if let Some(key) = path_key(expression) {
        let root = super::stmt::root_of(&key);
        let index = fx
            .scopes
            .iter()
            .rposition(|scope| scope.vars.contains_key(root))
            .unwrap_or(0);
        if let Some(scope) = fx.scopes.get_mut(index) {
            scope
                .shared_narrowing_paths
                .insert(key, expression.is_shared_location(classes));
        }
    }
    for child in expression.children() {
        if let hir::HirChild::Expr(child) = child {
            register_shared_paths(child, classes, fx);
        }
    }
}
