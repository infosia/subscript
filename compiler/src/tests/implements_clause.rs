//! A class cannot declare `implements` (compiler.md §205).
//!
//! Cost: eight checker calls on one-file programs, with no code
//! generation and no external process; about 0.01 s in a debug build.

use super::*;
use crate::divergence::Divergence;

const MESSAGE: &str =
    "a class cannot declare `implements`: types are nominal (C1), so the clause gives no conversion";

fn check(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::entry("main.ts", src)])
}

/// Each case is a class declaration with the clause, and the same
/// declaration with no clause. `column` is the 1-based column of the
/// first implemented name on line 3.
const CASES: &[(&str, &str, u32)] = &[
    (
        "reference class",
        "class B implements A { y: i32 = 2; }",
        20,
    ),
    (
        "value class",
        "@ValueType class B implements A { y: i32 = 2; }",
        31,
    ),
    (
        "generic class",
        "class B<T> implements A { y: i32 = 2; }",
        23,
    ),
    (
        "clause with two names",
        "class B implements A, C { y: i32 = 2; }",
        20,
    ),
];

fn program(declaration: &str) -> String {
    format!(
        "class A {{ x: i32 = 1; }}\nclass C {{ z: i32 = 3; }}\n{declaration}\n\
         export function main(): void {{ print(`${{new A().x}}`); }}\n"
    )
}

#[test]
fn an_implements_clause_is_rejected_at_the_clause() {
    for (name, declaration, column) in CASES {
        let diagnostics = check(&program(declaration)).expect_err(name);
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:?}");
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code, RuleCode::S100, "{name}");
        assert_eq!(diagnostic.message, MESSAGE, "{name}");
        assert_eq!(
            (diagnostic.pos.line, diagnostic.pos.col),
            (3, *column),
            "{name}"
        );
        assert_eq!(
            diagnostic.divergence,
            Some(Divergence::ClassImplementsClause),
            "{name}"
        );
    }
}

#[test]
fn the_same_class_with_no_clause_is_accepted() {
    for (name, declaration, _) in CASES {
        let plain = declaration
            .replace(" implements A, C", "")
            .replace(" implements A", "");
        assert!(!plain.contains("implements"), "{name}");
        assert!(check(&program(&plain)).is_ok(), "{name}: {plain}");
    }
}
