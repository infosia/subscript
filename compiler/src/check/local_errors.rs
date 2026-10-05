//! Record error-typed locals under the body check substitution.

use super::{Checker, FnCtx, Local, Scope};
use crate::types::Type;

impl Checker<'_> {
    /// Declares a local with its diagnostic-suppression status.
    pub(super) fn declare_in_context(&self, name: &str, local: Local, fx: &mut FnCtx) -> bool {
        let rejected = self.apparent_type(&local.ty) == Type::Error;
        fx.declare(name, local, rejected)
    }
}

impl Scope {
    /// Inserts a local and updates the names for diagnostic suppression.
    pub(super) fn insert_local(&mut self, name: String, local: Local, rejected: bool) {
        if rejected {
            self.rejected_local_names.insert(name.clone());
        } else if self.rejected_local_names.contains(&name) {
            self.rejected_local_names.remove(&name);
        }
        self.vars.insert(name, local);
    }
}
