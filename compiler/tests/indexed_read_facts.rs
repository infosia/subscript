//! Element checks select diagnostics without changing types (compiler.md §163).
use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

fn member(body: &str) -> Diagnostic {
    let source = format!(
        "class A {{ x: i32 = 1; }}
         class H {{ xs: (A | null)[] = []; }}
         function touch(): void {{}}
         function mk(): A | null {{ return null; }}
         export function f(xs: (A | null)[], h: H, j: i32): void {{ {body} }}"
    );
    let diagnostics = check_program(&[SourceFile::entry("test.ts", source)]).unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{body}: {diagnostics:?}");
    let diagnostic = diagnostics.into_iter().next().unwrap();
    assert_eq!(diagnostic.code, RuleCode::S011, "{body}");
    assert_eq!(diagnostic.message, "`A | null` may be null here; copy the element to a `const` local and test the local\nnote: const v = xs[i]; if (v !== null) { v.x }");
    diagnostic
}

fn pair(checked: &str, unchecked: &str) {
    assert_eq!(
        member(checked).divergence,
        Some(Divergence::IndexedReadNullCheck)
    );
    assert_eq!(member(unchecked).divergence, None);
}

#[test]
fn literal_and_const_keys_select_only_the_checked_element() {
    pair(
        "if (xs[0] !== null) { print(`${xs[0].x}`); }",
        "if (xs[0] !== null) { print(`${xs[1].x}`); }",
    );
    pair(
        "const i = 0; if (xs[i] !== null) { print(`${xs[i].x}`); }",
        "let i = 0; if (xs[i] !== null) { print(`${xs[i].x}`); }",
    );
    pair(
        "const i = 0; if (xs[i] !== null) { print(`${xs[i].x}`); }",
        "const i = 0; const k = 1; if (xs[i] !== null) { print(`${xs[k].x}`); }",
    );
    pair(
        "if (xs[0] !== null) { print(`${xs[0].x}`); }",
        "if (xs[0 + 0] !== null) { print(`${xs[0 + 0].x}`); }",
    );
    pair(
        "if (xs[0] !== null) { print(`${xs[0].x}`); }",
        "print(`${xs[0].x}`);",
    );
}

#[test]
fn const_key_identity_distinguishes_shadowed_bindings() {
    pair(
        "const i = 0; if (xs[i] !== null) { { const k = 0; } print(`${xs[i].x}`); }",
        "const i = 0; if (xs[i] !== null) { { const i = 1; print(`${xs[i].x}`); } }",
    );
}

#[test]
fn calls_and_other_or_unknown_stores_keep_element_checks() {
    for store in ["touch();", "xs[1] = null;", "xs[j] = null;"] {
        pair(
            &format!("if (xs[0] !== null) {{ {store} print(`${{xs[0].x}}`); }}"),
            &format!("if (xs[1] !== null) {{ {store} print(`${{xs[0].x}}`); }}"),
        );
    }
}

#[test]
fn same_element_and_receiver_stores_end_checks() {
    for store in ["xs[0] = null;", "xs = [];", "xs[0] = new A();"] {
        pair(
            "if (xs[0] !== null) { print(`${xs[0].x}`); }",
            &format!("if (xs[0] !== null) {{ {store} print(`${{xs[0].x}}`); }}"),
        );
    }
    pair(
        "if (h.xs[0] !== null) { print(`${h.xs[0].x}`); }",
        "if (h.xs[0] !== null) { h.xs = []; print(`${h.xs[0].x}`); }",
    );
}

#[test]
fn loop_keys_and_back_edge_stores_follow_their_own_ends() {
    pair(
        "const i = 0; if (xs[i] !== null) { print(`${xs[i].x}`); }",
        "for (let i = 0; i < 2; i++) { if (xs[i] !== null) { print(`${xs[i].x}`); } }",
    );
    pair("if (xs[0] !== null) { for (let n = 0; n < 2; n++) { print(`${xs[0].x}`); xs[1] = null; } }",
         "if (xs[0] !== null) { for (let n = 0; n < 2; n++) { print(`${xs[0].x}`); xs[0] = null; } }");
    pair("if (h.xs[0] !== null) { for (let n = 0; n < 2; n++) { print(`${h.xs[0].x}`); touch(); } }",
         "if (h.xs[0] !== null) { for (let n = 0; n < 2; n++) { print(`${h.xs[0].x}`); h.xs = []; } }");
}

#[test]
fn negation_assignment_and_joins_keep_only_proven_checks() {
    pair(
        "if (!(xs[0] === null)) { print(`${xs[0].x}`); }",
        "if (!(xs[0] !== null)) { print(`${xs[0].x}`); }",
    );
    pair(
        "if ((xs[0] = mk()) !== null) { print(`${xs[0].x}`); }",
        "xs[0] = mk(); print(`${xs[0].x}`);",
    );
    pair(
        "if ((xs[0] = new A()) !== null) { print(`${xs[0].x}`); }",
        "xs[0] = new A(); print(`${xs[0].x}`);",
    );
    pair(
        "if (xs[0] === null) { return; } print(`${xs[0].x}`);",
        "if (xs[0] !== null) { touch(); } print(`${xs[0].x}`);",
    );
    pair("if (xs[0] !== null) { if (j === 0) { touch(); } else { xs[1] = null; } print(`${xs[0].x}`); }",
         "if (xs[0] !== null) { if (j === 0) { touch(); } else { xs[0] = null; } print(`${xs[0].x}`); }");
}

#[test]
fn indexed_arguments_stores_and_calls_use_the_copy_diagnostic() {
    for (declarations, ty, read, code) in [
        (
            "class A { x:i32=1; } function take(a:A):void{}",
            "A",
            "take(xs[0]);",
            RuleCode::S005,
        ),
        (
            "class A { x:i32=1; }",
            "A",
            "const a:A=xs[0];",
            RuleCode::S005,
        ),
        (
            "class A { x:i32=1; }",
            "A",
            "let a:A=new A(); a=xs[0];",
            RuleCode::S005,
        ),
        ("", "((x:i32)=>i32)", "xs[0](1);", RuleCode::S100),
        (
            "function take(f:()=>void):void{}",
            "(()=>void)",
            "take(xs[0]);",
            RuleCode::S100,
        ),
        ("", "(()=>i32)", "const h:()=>i32=xs[0];", RuleCode::S100),
    ] {
        for checked in [false, true] {
            let body = if checked {
                format!("if(xs[0]!==null){{{read}}}")
            } else {
                read.into()
            };
            let source =
                format!("{declarations} export function f(xs:({ty}|null)[]):void{{{body}}}");
            let diagnostics = check_program(&[SourceFile::entry("test.ts", source)]).unwrap_err();
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code, code);
            assert_eq!(
                diagnostics[0].divergence,
                checked.then_some(Divergence::IndexedReadNullCheck)
            );
            assert!(diagnostics[0]
                .message
                .contains("copy the element to a `const` local and test the local"));
        }
    }
}

#[test]
fn a_const_copy_narrows_for_every_key() {
    for key in ["0", "j", "j + 0"] {
        let source = format!("class A {{ x:i32=1; }} function f(xs:(A|null)[], j:i32):void{{ const v=xs[{key}]; if(v!==null){{print(`${{v.x}}`);}} }}");
        check_program(&[SourceFile::new("test.ts", source)]).unwrap();
        let direct = member(&format!(
            "if (xs[{key}] !== null) {{ print(`${{xs[{key}].x}}`); }}"
        ));
        assert_eq!(
            direct.divergence,
            (key == "0").then_some(Divergence::IndexedReadNullCheck)
        );
    }
}

#[test]
fn destructured_const_bindings_have_distinct_key_identities() {
    pair(
        "const [i, k] = [0, 0]; if (xs[i] !== null) { print(`${xs[i].x}`); }",
        "const [i, k] = [0, 0]; if (xs[i] !== null) { print(`${xs[k].x}`); }",
    );
}

#[test]
fn another_receiver_store_keeps_the_exact_receiver_check() {
    pair(
        "const other = new H(); if (h.xs[0] !== null) { other.xs = []; print(`${h.xs[0].x}`); }",
        "const other = new H(); if (h.xs[0] !== null) { h.xs = []; print(`${h.xs[0].x}`); }",
    );
}

#[test]
fn switch_breaks_keep_the_receiver_binding_of_each_exit() {
    pair("switch (j) { case 0: { const other: (A | null)[] = []; if (xs[0] === null) { return; } break; } default: if (xs[0] === null) { return; } } print(`${xs[0].x}`);",
         "switch (j) { case 0: { const xs: (A | null)[] = []; if (xs[0] === null) { return; } break; } default: if (xs[0] === null) { return; } } print(`${xs[0].x}`);");
}

#[test]
fn disposal_at_a_loop_break_keeps_the_element_check() {
    for checked in [true, false] {
        let guard = if checked {
            "if (xs[0] === null) { return; }"
        } else {
            ""
        };
        let source = format!("class A {{ x:i32=1; }} class R {{ [Symbol.dispose]():void{{}} }} function f(xs:(A|null)[]):void{{ {guard} while(true){{ using r=new R(); break; }} print(`${{xs[0].x}}`); }}");
        let diagnostics = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].divergence,
            checked.then_some(Divergence::IndexedReadNullCheck)
        );
    }
}

#[test]
fn integer_const_keys_share_literal_and_alias_identities() {
    for declaration in [
        "const i = 0;",
        "const i:i32 = 0;",
        "const k = 0; const i = k;",
    ] {
        pair(
            &format!("{declaration} if (xs[0] !== null) {{ print(`${{xs[i].x}}`); }}"),
            &format!("{declaration} if (xs[1] !== null) {{ print(`${{xs[i].x}}`); }}"),
        );
        for (checked, stored, read) in [("0", "i", "0"), ("i", "0", "i"), ("i", "i", "0")] {
            pair(
                &format!("{declaration} if (xs[{checked}] !== null) {{ xs[1] = null; print(`${{xs[{read}].x}}`); }}"),
                &format!("{declaration} if (xs[{checked}] !== null) {{ xs[{stored}] = null; print(`${{xs[{read}].x}}`); }}"),
            );
        }
    }
    pair(
        "const i = 0; const k = 0; if (xs[i] !== null) { print(`${xs[k].x}`); }",
        "const i = 0 + 0; if (xs[0] !== null) { print(`${xs[i].x}`); }",
    );
    pair(
        "const i = 0 + 0; if (xs[i] !== null) { print(`${xs[i].x}`); }",
        "const i = 0 + 0; const k = i; if (xs[i] !== null) { print(`${xs[k].x}`); }",
    );
}
