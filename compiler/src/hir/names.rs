//! Source names and module labels for user-facing text (compiler.md §125).

use crate::Pos;

/// The identity of a declaration or a member in HIR (compiler.md §125 rule 2, §131).
///
/// A declaration symbol is unique in the program. A member symbol is unique
/// within its class. The full text of a built-in member symbol is the member
/// name, for example `push`, `next`, `constructor`, `get`, or `set`.
///
/// The representation is private. [`Symbol::source_name`] gives the source
/// spelling, and [`Symbol::full_text`] gives the complete identity text.
/// A symbol does not compare with a source name:
///
/// ```compile_fail,E0277
/// use subscript_compiler::hir::Callee;
/// fn find(callee: &Callee) -> bool {
///     matches!(callee, Callee::Func(s) if s == "name")
/// }
/// ```
///
/// ```compile_fail,E0277
/// use subscript_compiler::hir::Function;
/// fn same(symbol: &subscript_compiler::hir::Symbol, function: &Function) -> bool {
///     *symbol == function.name
/// }
/// ```
///
/// A consumer compares the source spelling, or compares two symbols:
///
/// ```
/// use subscript_compiler::hir::{Callee, Function};
/// fn find(callee: &Callee) -> bool {
///     matches!(callee, Callee::Func(s) if s.source_name() == "name")
/// }
/// fn calls(callee: &Callee, function: &Function) -> bool {
///     matches!(callee, Callee::Func(s) if *s == function.symbol)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(String);

impl Symbol {
    /// Makes a symbol from its complete identity text.
    ///
    /// The checker assigns every symbol. A consumer never builds a symbol
    /// from a source name: for a module function,
    /// `Symbol::from_full_text("main") == function.symbol` is always false.
    /// A consumer compares [`Symbol::source_name`], or compares two symbols
    /// that HIR carries.
    #[doc(hidden)]
    #[must_use]
    pub fn from_full_text(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The source spelling of the declaration, without identity markers.
    #[must_use]
    pub fn source_name(&self) -> String {
        source_name(&self.0)
    }

    /// The complete identity text. A declaration symbol's text is unique in
    /// the program; a member symbol's text is unique within its class.
    #[must_use]
    pub fn full_text(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.source_name())
    }
}

/// Source spelling retained beside a checker-assigned symbol.
pub fn source_name(symbol: &str) -> String {
    let mut result = String::new();
    let mut rest = symbol;
    while let Some(start) = rest.find("[[identity:") {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("]]") else {
            result.push_str(&rest[start..]);
            return result;
        };
        rest = &rest[start + end + 2..];
    }
    result.push_str(rest);
    result
}

/// Adds the module when declarations share a source name. An empty origin denotes the prelude.
pub fn declaration_label(name: &str, pos: &Pos, shared: bool) -> String {
    if shared {
        let file = if pos.file.is_empty() {
            "prelude/lang.d.ts"
        } else {
            &pos.file
        };
        format!("{name} ({file})")
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_qualify_shared_names_and_preserve_source_spelling() {
        assert_eq!(
            declaration_label("C", &Pos::new("lib.ts", 1, 1), false),
            "C"
        );
        assert_eq!(
            declaration_label("C", &Pos::new("lib.ts", 1, 1), true),
            "C (lib.ts)"
        );
        assert_eq!(
            declaration_label("Error", &Pos::new("", 0, 0), true),
            "Error (prelude/lang.d.ts)"
        );
        assert_eq!(
            source_name("[[identity:module:00]]pick<[[identity:module:01]]E>"),
            "pick<E>"
        );
        assert_eq!(source_name("plain"), "plain");
    }

    #[test]
    fn a_symbol_separates_its_source_name_from_its_identity() {
        let text = "[[identity:module:00]]pick<[[identity:module:01]]E>";
        let symbol = Symbol::from_full_text(text);
        assert_eq!(symbol.source_name(), "pick<E>");
        assert_eq!(symbol.to_string(), "pick<E>");
        assert_eq!(format!("`{symbol}`"), "`pick<E>`");
        assert_eq!(symbol.full_text(), text);
        let plain = Symbol::from_full_text("plain");
        assert_eq!(plain.source_name(), "plain");
        assert_eq!(plain.to_string(), "plain");
        assert_eq!(plain.full_text(), "plain");
        // Two identities of one source name stay two symbols.
        let other = Symbol::from_full_text("[[identity:module:02]]pick<[[identity:module:01]]E>");
        assert_eq!(other.source_name(), symbol.source_name());
        assert_ne!(other, symbol);
        assert_eq!(symbol.clone(), symbol);
        assert!(symbol < other);
    }
}
