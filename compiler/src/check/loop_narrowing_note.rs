//! Diagnostic provenance for narrowing facts ended at a loop head (§308 rule 1).
use super::{expr::path_key, stmt::root_of, FnCtx, NarrowingFact};
use crate::hir;

/// The scope identity prevents a hidden local with the same spelling from
/// inheriting the outer binding's loop-entry fact.
pub(super) type EndedLoopFacts = std::collections::HashMap<String, Option<usize>>;

impl FnCtx {
    fn path_scope(&self, key: &str) -> Option<usize> {
        let root = root_of(key);
        self.scopes
            .iter()
            .rposition(|scope| scope.vars.contains_key(root) || scope.pending.contains(root))
    }

    pub(super) fn record_loop_ended_fact(&mut self, fact: &NarrowingFact) {
        let scope = self.path_scope(&fact.key);
        if let Some(ended) = self.loop_ended_narrowing.last_mut() {
            ended.insert(fact.key.clone(), scope);
        }
    }

    /// A body store replaces the loop-head cause for this binding and its fields.
    fn end_loop_note_at_store(&mut self, key: &str) {
        let scope = self.path_scope(key);
        for ended in self.loop_ended_narrowing.iter_mut() {
            ended.retain(|path, saved| {
                *saved != scope || !(path == key || path.starts_with(&format!("{key}.")))
            });
        }
    }

    /// A body test replaces the loop-head cause for this exact path.
    pub(super) fn end_loop_note_at_test(&mut self, key: &str) {
        let scope = self.path_scope(key);
        for ended in self.loop_ended_narrowing.iter_mut() {
            if ended.get(key).is_some_and(|saved| *saved == scope) {
                ended.remove(key);
            }
        }
    }

    pub(super) fn end_loop_notes_at_stores(&mut self, effects: &hir::NarrowingEffects) {
        for key in &effects.stores {
            self.end_loop_note_at_store(key);
        }
    }

    /// The loop-head cause survives a join if an incoming path still sees it.
    pub(super) fn merge_loop_notes(&mut self, other: &[EndedLoopFacts]) {
        for (ended, incoming) in self.loop_ended_narrowing.iter_mut().zip(other) {
            ended.extend(incoming.iter().map(|(key, scope)| (key.clone(), *scope)));
        }
    }

    pub(super) fn loop_ended_path(&self, key: &str) -> bool {
        let scope = self.path_scope(key);
        self.loop_ended_narrowing
            .iter()
            .rev()
            .any(|ended| ended.get(key).is_some_and(|saved| *saved == scope))
    }

    pub(super) fn loop_ended_fact(&self, expression: &hir::Expr) -> bool {
        path_key(expression).is_some_and(|key| self.loop_ended_path(&key))
    }
}
