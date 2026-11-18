//! Checker controls for compiler.md §330 rules 1 to 4.
//! Measured cost: four checker tests take 0.07 s in debug.

use crate::{check_program, SourceFile};

fn accepted(source: &str) {
    check_program(&[SourceFile::new("s330-control.ts", source)])
        .unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
}

fn rejected(source: &str, message: &str) {
    let errors = check_program(&[SourceFile::new("s330-negative.ts", source)])
        .expect_err("the source must build a disjoint literal comparison");
    assert_eq!(errors.len(), 1, "{source}: {errors:?}");
    assert_eq!(errors[0].code.as_str(), "S100");
    assert_eq!(errors[0].message, message, "{source}");
    assert!(errors[0].divergence.is_none());
}

#[test]
fn literal_sources_reject_distinct_values_and_accept_equal_values() {
    for (declarations, operand, equal, different) in [
        ("", "1", "1", "2"),
        ("const n = -1;", "n", "-1", "1"),
        ("const n = (0x10);", "(n)", "16", "17"),
        ("const n = 1e2;", "n", "100", "101"),
        ("const n = 1.5;", "n", "1.5", "2.5"),
        ("const n = true;", "n", "true", "false"),
        ("const n = \"a\";", "n", "\"a\"", "\"b\""),
        ("const n = `a`;", "n", "\"a\"", "\"b\""),
        ("const a = 1; const n = a;", "n", "1", "2"),
        (
            "class K { static readonly Z = 0; } const n = K.Z;",
            "n",
            "0",
            "1",
        ),
        (
            "class K { readonly size = 4; } const k = new K();",
            "k.size",
            "4",
            "5",
        ),
        (
            "class K { readonly f = true; } const k = new K();",
            "k.f",
            "true",
            "false",
        ),
        (
            "class K { readonly a = 4; readonly b = this.a; } const k = new K();",
            "k.b",
            "4",
            "5",
        ),
    ] {
        for op in ["===", "!==", "==", "!="] {
            let expected = format!("comparison of different literals {equal} and {different}");
            rejected(
                &format!("{declarations} function f(): void {{ {operand} {op} {different}; }}"),
                &expected,
            );
            accepted(&format!(
                "{declarations} function f(): void {{ {operand} {op} {equal}; }}"
            ));
        }
    }
    rejected(
        "function f(): void { const n = 3; const m = n; m === 4; }",
        "comparison of different literals 3 and 4",
    );
    accepted("function f(): void { const n = 3; const m = n; m === 3; }");
    rejected("class K { readonly size = 4; } function k(): K { return new K(); } function f(): void { k().size === 5; }", "comparison of different literals 4 and 5");
    accepted("class K { readonly size = 4; } function k(): K { return new K(); } function f(): void { k().size === 4; }");
}

#[test]
fn declarations_and_operator_results_keep_their_widened_types() {
    for source in [
        "const n: i32 = 1; function f(): void { n === 2; }",
        "let n = 1; function f(): void { n === 2; }",
        "function f(n: i32): void { n === 2; }",
        "function n(): i32 { return 1; } function f(): void { n() === 2; }",
        "const n = 1 + 0; function f(): void { n === 2; }",
        "class K { readonly n: i32 = 1; } function f(k: K): void { k.n === 2; }",
        "class K { n = 1; } function f(k: K): void { k.n === 2; }",
        "const n = 1; function f(): void { let m = n; m === 2; }",
        "const n = 1; function f(n: i32): void { n === 2; }",
        "const n = 1; function f(): void { let n = 1; n === 2; }",
        "const n = 1; function f(): void { n < 2; n <= 2; n > 2; n >= 2; }",
    ] {
        accepted(source);
    }
    rejected(
        "const n = 1; function f(): void { n === 2; }",
        "comparison of different literals 1 and 2",
    );
}

#[test]
fn switch_labels_compare_literal_values_before_alias_conversion() {
    for (declarations, subject, equal, different) in [
        ("const n = 1;", "n", "1", "2"),
        ("const n = -1;", "n", "-1", "1"),
        ("const n = \"a\";", "n", "\"a\"", "\"b\""),
        ("class K { static readonly Z = 0; }", "K.Z", "0", "1"),
        (
            "class K { readonly n = 1; } const k = new K();",
            "k.n",
            "1",
            "2",
        ),
    ] {
        rejected(&format!("{declarations} function f(): void {{ switch ({subject}) {{ case {equal}: break; case {different}: break; }} }}"),
            &format!("case literal {different} differs from subject literal {equal}"));
        accepted(&format!(
            "{declarations} function f(): void {{ switch ({subject}) {{ case {equal}: break; }} }}"
        ));
    }
    accepted("const n: i32 = 1; function f(): void { switch (n) { case 2: break; } }");
    accepted("type Mode = \"a\" | \"b\"; function f(m: Mode): void { const n = \"a\"; m === n; switch (m) { case n: break; default: break; } }");
}

#[test]
fn prior_own_value_tests_exempt_only_the_same_binding_in_enclosing_bodies() {
    for body in [
        "n === 1 || n === 2;",
        "n !== 1 && n === 2;",
        "1 === n || n === 2;",
        "if (n === 1) {} else { n === 2; }",
        "if (n === 1) { return; } n === 2;",
        "if (n === 1) {} n === 2;",
        "print(`${n === 1}`); n === 2;",
        "n === 1 ? true : (n === 2 ? false : true);",
        "if (n === 1) { return; } switch (n) { case 2: break; }",
        "switch (n) { case 1: break; default: n === 2; }",
        "switch (n) { case 1: n === 2; break; }",
        "n === 1; { n === 2; }",
    ] {
        accepted(&format!("const n = 1; function f(): void {{ {body} }}"));
    }
    for body in [
        "n === 2; n === 1;",
        "{ n === 1; } n === 2;",
        "if (true) { n === 1; } else { n === 2; }",
        "const m = 1; m === 1; n === 2;",
        "n === 1; { const n = 1; n === 2; }",
        "1 === 1; n === 2;",
        "n === 1; 1 === 2;",
    ] {
        rejected(
            &format!("const n = 1; function f(): void {{ {body} }}"),
            "comparison of different literals 1 and 2",
        );
    }
    rejected(
        "const n = 1; function f(): void { switch (n) { case 1: break; case 2: break; } }",
        "case literal 2 differs from subject literal 1",
    );
    rejected(
        "const n = 1; function f(): void { switch (n) { case 1: n === 1; break; case 2: break; } }",
        "case literal 2 differs from subject literal 1",
    );
    rejected(
        "const n = 1; function f(): void { n === 1; } function g(): void { n === 2; }",
        "comparison of different literals 1 and 2",
    );
    rejected("const n = 1; function f(): void { switch (n) { case 1: switch (n) { case 2: break; } break; } }", "case literal 2 differs from subject literal 1");
    accepted("const n = 1; function f(): void { switch (n) { case 1: n === 1; switch (n) { case 2: break; } break; } }");
    accepted("function f(): void { const n = 1; n === 1 || n === 2; }");
    accepted("class K { static readonly Z = 0; } function f(): void { K.Z === 0 || K.Z === 1; }");
    accepted(
        "class K { readonly size = 4; } function f(k: K): void { k.size === 4 || k.size === 5; }",
    );
    rejected("class K { readonly size = 4; } function f(k: K, other: K): void { k.size === 4; other.size === 5; }", "comparison of different literals 4 and 5");
    accepted("const f = true; function g(): void { f === true || f === false; }");
    accepted("const s = \"a\"; function g(): void { s === \"a\" || s === \"b\"; }");
}
