//! The one point where a type argument enters a container type
//! (`specs/blocks/compiler.md` §132).
//!
//! Every form of `Map<K, V>`, `Set<T>`, and an array element that takes
//! its argument from the program passes the argument through
//! [`Checker::container_argument`]: a type annotation, the type
//! arguments of `new`, an inferred element, and a generic substitution
//! (which resolves an annotation with the parameter bound).

use super::*;

/// The position that a type argument fills in a container type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContainerSlot {
    /// The element of `T[]`, `Array<T>`, or `FixedArray<T, N>`.
    ArrayElement,
    /// The key of `Map<K, V>`.
    MapKey,
    /// The value of `Map<K, V>`.
    MapValue,
    /// The element of `Set<T>`.
    SetElement,
}

impl<'p> Checker<'p> {
    /// Admits `argument` into a container type at `slot`
    /// (compiler.md §40.1, §132 rule 1).
    ///
    /// A Context-affine argument reports the §40.1 S100 at `pos` and
    /// comes back as [`Type::Error`]. The container type constructor
    /// then gives the error type, so no later check reports the same
    /// failure (§132 rules 2 and 2a). While
    /// [`Checker::in_poisoned_context`] is set, the declaration that
    /// gives the context reported the failure, and this form reports
    /// nothing.
    pub(crate) fn container_argument(
        &mut self,
        slot: ContainerSlot,
        argument: Type,
        pos: Pos,
    ) -> Type {
        if self.instance_restriction(opaque::InstanceRestriction::ContainerArgument, &argument)
            || !Self::is_context_affine_type(&argument)
        {
            return argument;
        }
        if !self.in_poisoned_context {
            match slot {
                ContainerSlot::ArrayElement => self.error(
                    RuleCode::S100,
                    "Worker, Inbox, and Outbox values may not be array elements",
                    pos,
                ),
                ContainerSlot::MapKey | ContainerSlot::MapValue | ContainerSlot::SetElement => self
                    .error_diverging(
                        RuleCode::S100,
                        "Worker, Inbox, and Outbox values may not be container type arguments",
                        pos,
                        Divergence::WorkerContextAffinity,
                    ),
            }
        }
        Type::Error
    }

    /// Sets [`Checker::in_poisoned_context`] when the contextual type
    /// `context` is the error type (§132 rule 2), and returns the
    /// previous value for [`Checker::leave_container_context`].
    pub(crate) fn enter_container_context(&mut self, context: Option<&Type>) -> bool {
        let saved = self.in_poisoned_context;
        self.in_poisoned_context = saved || matches!(context, Some(Type::Error));
        saved
    }

    /// Clears [`Checker::in_poisoned_context`] for the check of a
    /// generic instance, and returns the previous value for
    /// [`Checker::leave_container_context`].
    ///
    /// The poisoned context covers the type arguments of one
    /// construction. An instance body that the arguments name is checked
    /// once and cached, so it reports its own failures for every later
    /// use (§132 rule 2).
    pub(crate) fn suspend_container_context(&mut self) -> bool {
        std::mem::replace(&mut self.in_poisoned_context, false)
    }

    /// Restores the value that [`Checker::enter_container_context`] or
    /// [`Checker::suspend_container_context`] returned.
    pub(crate) fn leave_container_context(&mut self, saved: bool) {
        self.in_poisoned_context = saved;
    }
}
