//! Shared diagnostic messages and rules. Site examples live in rejection_examples (§311).

pub(crate) const REACTION_RECEIVER_RULE: &str =
    "A reaction callback owns const captures. Copy the needed field into a const.";

pub(crate) const RECEIVER_MESSAGE: &str = "an async arrow cannot capture `this` directly; copy the receiver into a const or use a class field with an async method";
pub(crate) const RECEIVER_RULE: &str = "An async arrow owns const captures. A direct this capture remains outside the accepted surface.";

/// The places where the token `undefined` is legal (compiler.md §250
/// rule 4): the result of a function with the result `T | undefined` or
/// `void`, and an absence comparison.
pub(crate) const UNDEFINED_PLACES: &str =
    "`undefined` is legal only as a result `T | undefined`, as a result `void`, and in an absence comparison";
