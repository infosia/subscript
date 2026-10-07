//! Recursive ownership descriptions (§171 rules 1 and 7).

use super::Type;

/// The owner operation derived from a counted language type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CountedType {
    /// A registered async frame.
    Handle,
    /// A generator frame with state-specific cleanup.
    Generator,
    /// A reference to an array that owns its elements.
    Array(Box<CountedType>),
    /// Inline element owners.
    FixedArray(Box<CountedType>, u32),
    /// An inline generator result that owns its value.
    IterResult(Box<CountedType>),
}

impl Type {
    /// Derives the recursive owner operation, or returns `None` for an uncounted type.
    pub fn counted_type(&self) -> Option<CountedType> {
        match self {
            Self::AsyncHandle(_) => Some(CountedType::Handle),
            Self::Generator(_) => Some(CountedType::Generator),
            Self::Array(element) => element
                .counted_type()
                .map(|element| CountedType::Array(Box::new(element))),
            Self::FixedArray(element, count) => element
                .counted_type()
                .map(|element| CountedType::FixedArray(Box::new(element), *count)),
            Self::IterResult(value) => value
                .counted_type()
                .map(|value| CountedType::IterResult(Box::new(value))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counted_description_covers_every_holder_at_arbitrary_depth() {
        let handle = Type::AsyncHandle(Box::new(Type::I32));
        assert_eq!(handle.counted_type(), Some(CountedType::Handle));
        assert_eq!(
            Type::Generator(Box::new(Type::I32)).counted_type(),
            Some(CountedType::Generator)
        );
        assert_eq!(
            crate::lir::GeneratorCleanup::new(None, vec![crate::lir::ValueId(0)]).owners,
            vec![crate::lir::ValueId(0)]
        );
        let mut ty = handle;
        let mut description = CountedType::Handle;
        for _ in 0..8 {
            ty = Type::Array(Box::new(ty));
            description = CountedType::Array(Box::new(description));
            assert_eq!(ty.counted_type(), Some(description.clone()));
        }
        ty = Type::FixedArray(Box::new(ty), 3);
        description = CountedType::FixedArray(Box::new(description), 3);
        assert_eq!(ty.counted_type(), Some(description.clone()));
        ty = Type::IterResult(Box::new(ty));
        assert_eq!(
            ty.counted_type(),
            Some(CountedType::IterResult(Box::new(description)))
        );
    }

    #[test]
    fn uncounted_children_do_not_create_owners() {
        for ty in [
            Type::I32,
            Type::Array(Box::new(Type::I32)),
            Type::FixedArray(Box::new(Type::I32), 2),
            Type::IterResult(Box::new(Type::I32)),
        ] {
            assert_eq!(ty.counted_type(), None);
        }
        // The fulfilled type does not change the frame's owner operation.
        assert_eq!(
            Type::AsyncHandle(Box::new(Type::Array(Box::new(Type::I32)))).counted_type(),
            Some(CountedType::Handle)
        );
    }
    #[test]
    fn lir_count_action_carries_the_recursive_release_storage() {
        use crate::lir::CountAction;
        assert_eq!(CountAction::for_type(&Type::I32), CountAction::Uncounted);
        assert_eq!(CountAction::Uncounted.release_type(), None);
        let handle = Type::AsyncHandle(Box::new(Type::I32));
        let result = Type::IterResult(Box::new(Type::FixedArray(
            Box::new(Type::Array(Box::new(handle))),
            2,
        )));
        let canonical = Type::IterResult(Box::new(Type::FixedArray(
            Box::new(Type::Array(Box::new(Type::AsyncHandle(Box::new(
                Type::Void,
            ))))),
            2,
        )));
        let action = CountAction::for_type(&result);
        assert_eq!(action.release_type(), Some(canonical));
        assert_eq!(
            action,
            CountAction::Counted(result.counted_type().expect("counted shape"))
        );
    }
}
