//! §321 case labels. Measured cost: 0.01 s in debug; two checker calls.
use subscript_compiler::{check_program, SourceFile};

#[test]
fn other_alias_and_number_labels_stay_rejected() {
    for label in ["other", "number"] {
        let source = format!(
            r#"
            type Mode = "aaa" | "bbb";
            type Other = "aaa" | "ccc";
            function f(m: Mode, other: Other, number: i32): void {{
                switch (m) {{ case {label}: break; default: break; }}
            }}
        "#
        );
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains(
            "case labels for string-literal union alias `Mode` must be string literals naming a member"
        )), "{errors:?}");
    }
}

// Measured cost: below 0.01 s in debug; two checker calls and HIR reads.
#[test]
fn mixed_labels_keep_integer_and_string_comparisons() {
    use subscript_compiler::hir::{BinOp, ExprKind, Stmt};
    for alias in [r#""aaa" | "bbb""#, "CEnum<{ aaa: -3; bbb: 7; }>"] {
        let source = format!(
            r#"type Mode = {alias};
            function f(m: Mode, s: string): void {{
                switch (m) {{ case "aaa": break; case s: break; default: break; }}
            }}"#
        );
        let module = check_program(&[SourceFile::new("test.ts", source)]).unwrap();
        let function = module.functions.iter().find(|f| f.name == "f").unwrap();
        let switch = function
            .body
            .iter()
            .find_map(|s| match s {
                Stmt::Switch { cases, .. } => Some(cases),
                _ => None,
            })
            .unwrap();
        for (index, expected) in [BinOp::Eq, BinOp::StrEq].iter().enumerate() {
            assert!(matches!(&switch[index].test.as_ref().unwrap().kind,
                ExprKind::Binary { op, .. } if op == expected));
        }
    }
}
