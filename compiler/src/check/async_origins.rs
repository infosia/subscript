//! Origin facts for conditional values and parameter defaults (§317).

use std::collections::{HashMap, HashSet};

use super::{rejection::RejectionSite, FnCtx};
use crate::diag::Pos;
use crate::hir;

#[derive(Clone, Copy, Debug)]
enum OriginUse {
    Made,
    Selected { every_operand: bool },
}

impl OriginUse {
    fn discharges(self) -> bool {
        matches!(
            self,
            Self::Made
                | Self::Selected {
                    every_operand: true
                }
        )
    }
}

/// Each origin retains whether an operand makes it or selects it, and
/// whether every possible value carries an already selected origin.
#[derive(Clone, Debug, Default)]
pub(crate) struct AsyncOrigins(HashMap<u32, OriginUse>);

impl AsyncOrigins {
    pub(super) fn made(origin: u32) -> Self {
        Self(HashMap::from([(origin, OriginUse::Made)]))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Aggregate elements coexist; one certain occurrence suffices (§188).
    pub(super) fn extend(&mut self, other: Self) {
        for (id, usage) in other.0 {
            self.0
                .entry(id)
                .and_modify(|held| {
                    if usage.discharges() {
                        *held = usage;
                    }
                })
                .or_insert(usage);
        }
    }

    pub(super) fn conditional(left: Self, right: Self) -> Self {
        let mut result = Self::default();
        for (&id, &usage) in left.0.iter().chain(&right.0) {
            let usage = match usage {
                OriginUse::Made => OriginUse::Made,
                OriginUse::Selected { .. } => OriginUse::Selected {
                    every_operand: left.0.get(&id).is_some_and(|u| u.discharges())
                        && right.0.get(&id).is_some_and(|u| u.discharges()),
                },
            };
            result.0.insert(id, usage);
        }
        result
    }

    fn selected(mut self) -> Self {
        for usage in self.0.values_mut() {
            *usage = OriginUse::Selected {
                every_operand: usage.discharges(),
            };
        }
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AsyncOrigin {
    pub(super) pos: Pos,
    pub(super) handled: bool,
    parameter: bool,
    conditional: bool,
    conditional_return: bool,
    parameter_default: bool,
}

impl AsyncOrigin {
    pub(super) fn rejection_site(&self) -> RejectionSite {
        if self.parameter_default {
            RejectionSite::AsyncDefaultReadsParameter
        } else if self.conditional_return && self.parameter {
            RejectionSite::AsyncConditionalReturnParameter
        } else if self.conditional_return {
            RejectionSite::AsyncConditionalReturnBinding
        } else if self.conditional && self.parameter {
            RejectionSite::AsyncConditionalParameter
        } else if self.conditional {
            RejectionSite::AsyncConditionalBinding
        } else {
            RejectionSite::AsyncHandleUnawaited
        }
    }
}

impl FnCtx<'_> {
    pub(crate) fn register_async_origin(&mut self, pos: Pos) -> u32 {
        let id = self.async_origins.len() as u32;
        self.async_origins.push(AsyncOrigin {
            pos,
            handled: false,
            parameter: false,
            conditional: false,
            conditional_return: false,
            parameter_default: false,
        });
        id
    }

    pub(super) fn register_async_parameter(&mut self, pos: Pos) -> u32 {
        let id = self.register_async_origin(pos);
        if let Some(origin) = self.async_origins.get_mut(id as usize) {
            origin.parameter = true;
        }
        id
    }

    pub(crate) fn handle_async_origins(&mut self, origins: &AsyncOrigins) {
        for (&id, &usage) in &origins.0 {
            if let Some(origin) = self.async_origins.get_mut(id as usize) {
                if usage.discharges() {
                    origin.handled = true;
                } else {
                    origin.conditional = true;
                }
            }
        }
    }

    pub(super) fn handle_return_async_origins(&mut self, origins: &AsyncOrigins) {
        self.handle_async_origins(origins);
        for (&id, &usage) in &origins.0 {
            if !usage.discharges() {
                if let Some(origin) = self.async_origins.get_mut(id as usize) {
                    origin.conditional_return = true;
                }
            }
        }
    }

    /// A default cannot transfer origins carried by earlier parameters of
    /// the same function (compiler.md §329 rule 3, §317 rule 2).
    pub(super) fn parameter_default_origins(
        &mut self,
        mut origins: AsyncOrigins,
        parameters: &[hir::Param],
    ) -> AsyncOrigins {
        if origins.is_empty() {
            return origins;
        }
        let earlier_origins: HashSet<_> = parameters
            .iter()
            .flat_map(|parameter| self.local_async_origins(&parameter.name).0.into_keys())
            .collect();
        origins.0.retain(|id, _| {
            if earlier_origins.contains(id) {
                if let Some(origin) = self.async_origins.get_mut(*id as usize) {
                    origin.parameter_default = true;
                }
                false
            } else {
                true
            }
        });
        origins
    }

    pub(crate) fn local_async_origins(&self, name: &str) -> AsyncOrigins {
        if let Some(origins) = self.async_operand_origins.get(name) {
            return origins.clone();
        }
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
            .map_or_else(AsyncOrigins::default, |local| {
                local.async_origins.clone().selected()
            })
    }

    pub(crate) fn set_local_async_origins(&mut self, name: &str, origins: AsyncOrigins) {
        if let Some(local) = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.vars.get_mut(name))
        {
            local.async_origins = origins;
        }
    }
}
