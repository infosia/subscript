//! Lifetime parameter roles for runtime operations (compiler.md §120).

use crate::hir::{ArrFn, BuiltinMethod, MapFn, OperationSignatureTarget, SetFn};

impl OperationSignatureTarget {
    pub(crate) fn copies_lifetime_operand(&self, index: usize) -> bool {
        match self {
            Self::BuiltinMethod(BuiltinMethod::ArrayPush) => index == 1,
            Self::Arr(operation) => match operation {
                ArrFn::Fill | ArrFn::Unshift => index == 1,
                _ => false,
            },
            Self::Map(operation) => match operation {
                MapFn::Set => matches!(index, 1 | 2),
                _ => false,
            },
            Self::Set(SetFn::Add) => index == 1,
            Self::Ambient(_)
            | Self::ContextBytes(..)
            | Self::Math(_)
            | Self::Num(_)
            | Self::Date(_)
            | Self::Json(_)
            | Self::Text(_)
            | Self::Str(_)
            | Self::Regex(_)
            | Self::Set(_)
            | Self::Worker(_)
            | Self::BuiltinMethod(_) => false,
        }
    }
}

/// The HIR value named by a lifetime site (compiler.md §120).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifetimeOperand {
    /// The receiver or base of an access.
    Receiver,
    /// An index in the call argument list.
    Argument(usize),
}

impl LifetimeOperand {
    /// The index after an optional receiver prefix enters the operand list.
    #[must_use]
    pub fn evaluated_index(self, receiver_prefix: usize) -> usize {
        match self {
            Self::Receiver => 0,
            Self::Argument(index) => receiver_prefix + index,
        }
    }
}

impl crate::hir::Expr {
    /// The site for a statement that reads through this value (compiler.md §120.1 rule 3c).
    #[must_use]
    pub fn statement_read_sites(&self, module: &crate::hir::Module) -> Vec<crate::hir::TrapSite> {
        let classes = module
            .classes
            .iter()
            .map(crate::types::HandleClass::from)
            .collect::<Vec<_>>();
        if self
            .ty
            .handle_kind(&classes)
            .is_some_and(crate::types::HandleKind::needs_lifetime_trap)
        {
            vec![crate::hir::TrapSite::DevOnlyLifetime {
                operand: LifetimeOperand::Receiver,
                pos: self.pos.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}
