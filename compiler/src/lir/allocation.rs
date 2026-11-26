//! The allocation fact of each built-in method (`compiler.md` §342 rule 10).

use super::BuiltinMethod;

impl BuiltinMethod {
    /// Returns true if the method can allocate in the Context.
    ///
    /// The match is total, so a new method cannot exist without its fact.
    /// The interpreter verifies each "cannot allocate" value at run time
    /// (`compiler.md` §342 rule 10a).
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // A removal and a clear truncate in place. A reserved push
            // stores into capacity that exists before the loop (rule 10b).
            // An in-place `filter` stores into the buffer of its receiver
            // and shrinks its length (§359 rules 7 and 8).
            Self::ArrayPop
            | Self::ArrayClear
            | Self::ArrayPushReserved
            | Self::ArrayStoreKept
            | Self::ArrayTruncateKept => false,
            // A push can grow the storage, a slice makes a string, and a
            // generator resume runs script code.
            Self::ArrayPush | Self::StringSlice | Self::GeneratorNext => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removals_cannot_allocate_and_growth_can() {
        assert!(!BuiltinMethod::ArrayPop.can_allocate());
        assert!(!BuiltinMethod::ArrayClear.can_allocate());
        assert!(!BuiltinMethod::ArrayPushReserved.can_allocate());
        assert!(!BuiltinMethod::ArrayStoreKept.can_allocate());
        assert!(!BuiltinMethod::ArrayTruncateKept.can_allocate());
        assert!(BuiltinMethod::ArrayPush.can_allocate());
        assert!(BuiltinMethod::StringSlice.can_allocate());
        assert!(BuiltinMethod::GeneratorNext.can_allocate());
    }
}
