//! The positions of the `undefined` spelling (compiler.md §195.1 rule 6,
//! §195.6 rule 1).

use super::rejection::{diagnostic, RejectionSite};
use super::{Checker, ScopeItem};
use crate::diag::{Diagnostic, Pos};
use crate::hir;
use crate::hir::spelling::TypeSlot;
use crate::types::Type;

/// Rejects each type that holds the `undefined` spelling outside the whole
/// type of a value, a local, or a function result (§195.1 rule 6).
///
/// The walk reads every type slot of the module, so an inferred type
/// argument, an array element, a closure result, and inferred storage each
/// reach it. A position reports once.
pub(super) fn outside_value_diagnostics(module: &mut hir::Module) -> Vec<Diagnostic> {
    let mut reported: Vec<Pos> = Vec::new();
    let mut diagnostics = Vec::new();
    hir::spelling::for_each_type(module, &mut |ty, slot, pos| {
        let outside = match slot {
            TypeSlot::Expression | TypeSlot::Local | TypeSlot::FunctionResult => {
                ty.holds_nested_undefined()
            }
            TypeSlot::Storage => ty.holds_undefined(),
        };
        if outside && !reported.contains(pos) {
            reported.push(pos.clone());
            diagnostics.push(diagnostic(
                RejectionSite::UndefinedSpellingOutsideValue,
                "the `undefined` spelling is legal only as the whole type of a value, of a local, or of a function result",
                pos.clone(),
            ));
        }
    });
    diagnostics
}

impl Checker<'_> {
    /// Rejects a host entry whose signature holds the `undefined` spelling
    /// (§195.6 rule 1): C has one null pointer.
    pub(super) fn reject_undefined_host_entries(
        &mut self,
        exports: &[(String, ScopeItem, Pos, Option<Pos>)],
    ) {
        for (name, target, pos, _) in exports {
            let ScopeItem::Func(symbol) = target else {
                continue;
            };
            let Some(function) = self
                .functions
                .iter()
                .find(|function| function.symbol.full_text() == symbol)
            else {
                continue;
            };
            if self.apparent_type(&function.ret).holds_undefined()
                || function
                    .params
                    .iter()
                    .any(|param| self.apparent_type(&param.ty).holds_undefined())
            {
                let ret = self.type_name(&function.ret);
                self.reject_subset(
                    RejectionSite::UndefinedResultHostEntry,
                    format!(
                        "a function that crosses the C boundary has no `T | undefined`; entry export `{name}` returns `{ret}`"
                    ),
                    pos.clone(),
                );
            }
        }
    }

    /// Whether a function value of this signature result is rejected
    /// (§195.6 rule 2).
    pub(super) fn undefined_result_value(&mut self, result: &Type, pos: &Pos) -> bool {
        if !self.apparent_type(result).holds_undefined() {
            return false;
        }
        self.reject_subset(
            RejectionSite::UndefinedResultFunctionValue,
            "a function whose result is `T | undefined` may only be called",
            pos.clone(),
        );
        true
    }
}
