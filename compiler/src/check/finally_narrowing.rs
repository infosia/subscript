//! Causes of local facts ended by a finalizer on a jump edge (§315 rule 1b).
use super::{FinallyEndedFacts, FnCtx, NarrowingFacts};
use crate::hir;

impl<'p> FnCtx<'p> {
    pub(super) fn finally_ended_path(&self, key: &str) -> bool {
        self.finally_ended_narrowing
            .contains(&(key.to_owned(), self.path_scope(key)))
    }

    pub(super) fn replace_finally_causes_at_facts(&mut self) {
        if self.finally_ended_narrowing.is_empty() {
            return;
        }
        let replaced: Vec<_> = self
            .finally_ended_narrowing
            .iter()
            .filter(|(path, scope)| *scope == self.path_scope(path) && self.narrowed.contains(path))
            .cloned()
            .collect();
        for cause in replaced {
            self.finally_ended_narrowing.remove(&cause);
        }
    }

    pub(super) fn end_finally_causes_at_stores(&mut self, effects: &hir::NarrowingEffects) {
        if self.finally_ended_narrowing.is_empty() {
            return;
        }
        for key in &effects.stores {
            let scope = self.path_scope(key);
            self.finally_ended_narrowing.retain(|(path, saved)| {
                *saved != scope || !(path == key || path.starts_with(&format!("{key}.")))
            });
        }
    }

    /// A cause explains every incoming edge without the fact (§315 rule 1b).
    pub(super) fn joined_finally_causes<'a>(
        edges: impl IntoIterator<Item = (&'a NarrowingFacts, &'a FinallyEndedFacts)>,
    ) -> FinallyEndedFacts {
        let edges: Vec<_> = edges.into_iter().collect();
        let mut causes: FinallyEndedFacts = edges
            .iter()
            .flat_map(|(_, causes)| causes.iter().cloned())
            .collect();
        causes.retain(|cause| {
            edges
                .iter()
                .all(|(facts, causes)| facts.contains(&cause.0) || causes.contains(cause))
                && edges.iter().any(|(facts, _)| !facts.contains(&cause.0))
        });
        causes
    }

    pub(super) fn merge_finally_causes(
        &mut self,
        other: &FinallyEndedFacts,
        other_facts: &NarrowingFacts,
    ) {
        self.finally_ended_narrowing = Self::joined_finally_causes([
            (&self.narrowed, &self.finally_ended_narrowing),
            (other_facts, other),
        ]);
    }
}
