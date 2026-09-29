//! Synthesized function construction.

use super::{Function, Param, Stmt};
use crate::{Pos, Type};

impl Function {
    /// Builds a checker helper whose raise fact is derived with the module (compiler.md §119).
    #[must_use]
    pub fn new_synthesized_helper(
        name: String,
        params: Vec<Param>,
        ret: Type,
        body: Vec<Stmt>,
        pos: Pos,
    ) -> Self {
        Self {
            symbol: name.clone(),
            name,
            synthesized_helper: true,
            exported: false,
            is_generator: false,
            is_async: false,
            params,
            ret,
            body,
            can_raise: false,
            pos,
        }
    }
}
