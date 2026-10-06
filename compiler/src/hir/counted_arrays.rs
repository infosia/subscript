//! Counted array method classes from §171 rules 5 and 5a.

use super::ArrFn;

/// The ownership class of an array method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CountedArrayMethod {
    /// Reads an element without an owner transfer.
    Borrow,
    /// Inserts one element owner.
    Insert,
    /// Transfers removed element owners to a result.
    Remove,
    /// Copies elements into a fresh array.
    Copy,
    /// Acquires stored elements and releases replaced elements.
    Replace,
    /// Reorders elements and returns the borrowed receiver.
    Reorder,
    /// Rejects counted values in every callback position.
    Callback,
    /// Rejects a counted receiver element.
    Search,
}

impl ArrFn {
    /// Returns the method's counted-value class from the closed method table.
    #[must_use]
    pub fn counted_class(self) -> CountedArrayMethod {
        use CountedArrayMethod as C;
        match self {
            Self::At => C::Borrow,
            Self::Unshift => C::Insert,
            Self::Splice | Self::Shift => C::Remove,
            Self::Slice | Self::Concat => C::Copy,
            Self::Fill | Self::CopyWithin => C::Replace,
            Self::Reverse => C::Reorder,
            Self::ForEach
            | Self::Map
            | Self::Filter
            | Self::Reduce
            | Self::ReduceRight
            | Self::Some
            | Self::Every
            | Self::FindIndex
            | Self::Find
            | Self::FindLast
            | Self::FindLastIndex
            | Self::FlatMap
            | Self::Sort => C::Callback,
            Self::IndexOf | Self::LastIndexOf | Self::Includes | Self::Join => C::Search,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_method_has_the_contract_class() {
        use CountedArrayMethod as C;
        let expected = [
            C::Search,
            C::Search,
            C::Search,
            C::Search,
            C::Copy,
            C::Replace,
            C::Reorder,
            C::Copy,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Remove,
            C::Remove,
            C::Insert,
            C::Replace,
            C::Borrow,
            C::Callback,
            C::Callback,
            C::Callback,
            C::Callback,
        ];
        for (method, class) in ArrFn::ALL.into_iter().zip(expected) {
            assert_eq!(method.counted_class(), class, "{method:?}");
        }
    }
}
