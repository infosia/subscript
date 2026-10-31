//! A nullable string (`specs/blocks/compiler.md` §218).

use super::*;
use crate::divergence::Divergence;

const FIX: &str = "; a null test first gives a `string`";

fn with_function(function: &str) -> String {
    format!("{function}\nexport function main(): void {{}}\n")
}

fn mirror(declaration: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[
        SourceFile::ambient(
            "mirror.d.ts",
            format!("// @subscript-c-header include=\"x.h\"\n{declaration}\n"),
        ),
        SourceFile::entry("main.ts", "export function main(): void {}\n"),
    ])
}

/// The operator of the first `return a <op> b` of `f`.
fn returned_operator(module: &hir::Module) -> hir::BinOp {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("function f");
    match function.body.first() {
        Some(hir::Stmt::Return {
            value:
                Some(hir::Expr {
                    kind: hir::ExprKind::Binary { op, .. },
                    ..
                }),
            ..
        }) => *op,
        other => panic!("{other:?}"),
    }
}

#[test]
fn rule_1_accepts_each_position() {
    let source = "class C { a: string | null = null; }\n\
                  let g: string | null = null;\n\
                  function f(a: string | null): string | null { return a; }\n\
                  export function main(): void {\n\
                  \x20 const local: string | null = f(\"a\");\n\
                  \x20 const rows: (string | null)[] = [\"a\", null];\n\
                  \x20 const table = new Map<string, string | null>();\n\
                  \x20 table.set(\"a\", null);\n\
                  \x20 g = local;\n\
                  \x20 print(`${rows.length} ${new C().a === null}`);\n\
                  }\n";
    let module = check_one(source);
    assert!(module.is_ok(), "{:?}", module.err());
}

#[test]
fn rule_2_each_equality_of_a_string_form_selects_the_content_operator() {
    for (left, right) in [
        ("string | null", "string | null"),
        ("string | null", "string"),
        ("string", "string | null"),
        ("string", "string"),
    ] {
        for (operator, expected) in [
            ("===", hir::BinOp::StrEq),
            ("==", hir::BinOp::StrEq),
            ("!==", hir::BinOp::StrNe),
            ("!=", hir::BinOp::StrNe),
        ] {
            let source = with_function(&format!(
                "function f(a: {left}, b: {right}): boolean {{ return a {operator} b; }}"
            ));
            let module = check_one(&source).expect("an equality of strings");
            assert_eq!(returned_operator(&module), expected, "{left} {operator}");
        }
    }
    // Control: a null test of a nullable string reads no content.
    let module = check_one(&with_function(
        "function f(a: string | null): boolean { return a === null; }",
    ))
    .expect("a null test");
    assert_eq!(returned_operator(&module), hir::BinOp::Eq);
    // Control: a class equality is not a string equality.
    let module = check_one(&with_function(
        "class C {}\nfunction f(a: C, b: C): boolean { return a === b; }",
    ))
    .expect("a class equality");
    assert_eq!(returned_operator(&module), hir::BinOp::Eq);
}

#[test]
fn rule_2_the_definition_is_symmetric() {
    let nullable = Type::Nullable(Box::new(Type::Str), crate::types::Absence::Null);
    let forms = [Type::Str, nullable.clone(), Type::Null, Type::I32];
    for left in &forms {
        for right in &forms {
            assert_eq!(
                Type::compares_string_content(left, right),
                Type::compares_string_content(right, left),
                "{left:?} {right:?}"
            );
        }
    }
    assert!(Type::compares_string_content(&nullable, &Type::Str));
    assert!(!Type::compares_string_content(&Type::Str, &Type::Null));
    assert!(!Type::compares_string_content(&nullable, &Type::Null));
    assert!(!Type::compares_string_content(&Type::Str, &Type::I32));
}

#[test]
fn rule_5_each_text_states_the_fix() {
    for (form, code, message) in [
        (
            "function f(a: string | null): string { return `${a}`; }",
            RuleCode::S100,
            "type `string | null` cannot be interpolated into a template",
        ),
        (
            "function f(a: string | null): string { return a + \"x\"; }",
            RuleCode::S100,
            "operator not defined for `string | null` and `string`",
        ),
        (
            "function f(a: string | null): string { return \"x\" + a; }",
            RuleCode::S100,
            "operator not defined for `string` and `string | null`",
        ),
        (
            "function f(a: string | null): void { let s = \"x\"; s += a; }",
            RuleCode::S100,
            "operator `+=` is not defined for `string` and `string | null`",
        ),
        (
            "function f(a: string | null): void { print(a); }",
            RuleCode::S100,
            "type mismatch: the argument expects `string`, got `string | null`",
        ),
        (
            "function f(a: string | null): void { switch (a) { default: break; } }",
            RuleCode::S100,
            "switch discriminants are integers, enums, strings, or string-literal union aliases; got `string | null`",
        ),
    ] {
        let diagnostics = check_one(&with_function(form)).expect_err(form);
        assert_eq!(diagnostics[0].code, code, "{form}");
        assert_eq!(diagnostics[0].message, format!("{message}{FIX}"), "{form}");
    }
    // Controls: other operands keep their text, and a null test passes.
    let diagnostics = check_one(&with_function(
        "class C {}\nfunction f(a: C | null): string { return `${a}`; }",
    ))
    .expect_err("a nullable class");
    assert_eq!(
        diagnostics[0].message,
        "type `C | null` cannot be interpolated into a template"
    );
    let accepted = check_one(&with_function(
        "function f(a: string | null): string { if (a === null) { return \"\"; } return `${a}` + a; }",
    ));
    assert!(accepted.is_ok(), "{:?}", accepted.err());
}

#[test]
fn rule_6_an_unnarrowed_read_stays_rejected() {
    for statement in [
        "const n = a.length;",
        "const n = a[0];",
        "const n = a.toUpperCase();",
    ] {
        let source = with_function(&format!(
            "function f(a: string | null): void {{ {statement} }}"
        ));
        let diagnostics = check_one(&source).expect_err(statement);
        assert_eq!(diagnostics[0].code, RuleCode::S011, "{statement}");
    }
    let accepted = check_one(&with_function(
        "function f(a: string | null): void { if (a !== null) { const n = a.length; } }",
    ));
    assert!(accepted.is_ok(), "{:?}", accepted.err());
}

#[test]
fn rule_7_format_sort_and_key_forms_stay_rejected() {
    for (form, code) in [
        ("rows.join(\",\");", RuleCode::S014),
        ("rows.toString();", RuleCode::S014),
        ("rows.sort();", RuleCode::S014),
        ("new Set<string | null>();", RuleCode::S014),
        ("new Map<string | null, i32>();", RuleCode::S014),
    ] {
        let source = with_function(&format!(
            "function f(rows: (string | null)[]): void {{ {form} }}"
        ));
        let diagnostics = check_one(&source).expect_err(form);
        assert_eq!(diagnostics[0].code, code, "{form}");
    }
    // Control: a comparator sort is legal.
    let accepted = check_one(&with_function(
        "function f(rows: (string | null)[]): void { rows.sort((a: string | null, b: string | null): i32 => 0); }",
    ));
    assert!(accepted.is_ok(), "{:?}", accepted.err());
}

#[test]
fn rule_8_json_accepts_a_field() {
    let accepted = check_one(
        "class D { a: string | null = null; }\n\
         export function main(): void {\n\
         \x20 print(JSON.stringify(new D()));\n\
         \x20 const d = JSON.parse<D>(\"{\\\"a\\\":null}\");\n\
         \x20 print(`${d.a === null}`);\n\
         }\n",
    );
    assert!(accepted.is_ok(), "{:?}", accepted.err());
}

#[test]
fn rule_9_each_boundary_rejects_the_form() {
    for (declaration, name, ty) in [
        (
            "declare function f(a: string | null): void;",
            "f.a",
            "string | null",
        ),
        ("declare function f(): string | null;", "f", "string | null"),
        (
            "declare class C { a: string | null; constructor(a: string | null); }",
            "C.a",
            "string | null",
        ),
        (
            "// @subscript-c-callback typedef=\"Cb\"\ntype Cb = (a: string | null) => void;",
            "Cb",
            "(string | null) => void",
        ),
    ] {
        let diagnostics = mirror(declaration).expect_err(declaration);
        let expected = format!(
            "mirror `mirror.d.ts` declaration `{name}` has `{ty}`; the C ABI gives a nullable array or string no layout"
        );
        let found = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.message == expected)
            .unwrap_or_else(|| panic!("{declaration}: {diagnostics:?}"));
        assert_eq!(found.code, RuleCode::S100, "{declaration}");
    }
    let diagnostics = check_one(
        "class M { a: string | null = null; }\n\
         function echo(inbox: Inbox<M>, outbox: Outbox<M>): void {}\n\
         export function main(): void { const w: Worker<M, M> = Worker.spawn(echo); }\n",
    )
    .expect_err("a Worker message field");
    assert_eq!(
        diagnostics[0].message,
        "message class `M` is not transferable: innermost field `M.a` has non-transferable type `string | null`"
    );
    let diagnostics =
        check_one("export function main(a: string | null): void {}\n").expect_err("a host entry");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    // Control: a plain string view in a mirror needs its provenance, not this rule.
    let diagnostics = mirror("declare function f(a: string): void;").expect_err("no provenance");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("no layout")),
        "{diagnostics:?}"
    );
}

#[test]
fn rule_10_a_string_literal_alias_stays_rejected() {
    let diagnostics = check_one(&with_function(
        "type L = \"a\" | \"b\";\nfunction f(a: L | null): void {}",
    ))
    .expect_err("a literal alias");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::NullableNonReference)
    );
}
