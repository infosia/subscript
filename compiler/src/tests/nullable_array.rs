//! A nullable array (`specs/blocks/compiler.md` §217).

use super::*;
use crate::divergence::Divergence;

fn main_with(prelude: &str) -> String {
    format!("{prelude}\nexport function main(): void {{}}\n")
}

fn mirror(declaration: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[
        SourceFile::ambient("mirror.d.ts", format!("{declaration}\n")),
        SourceFile::entry("main.ts", "export function main(): void {}\n"),
    ])
}

#[test]
fn rule_1_accepts_an_array_of_each_element_that_holds_no_counted_value() {
    let source = main_with(
        "class P { x: i32 = 1; }\n\
         function a(x: i32[] | null): void {}\n\
         function b(x: P[] | null): void {}\n\
         function c(x: P[][] | null): void {}\n\
         function d(x: Map<string, Promise<void>>[] | null): void {}",
    );
    assert!(check_one(&source).is_ok());
}

#[test]
fn rule_2_rejects_a_counted_element_at_each_depth() {
    for form in [
        "Promise<void>[]",
        "Promise<i32>[]",
        "Promise<void>[][]",
        "FixedArray<Promise<void>, 2>[]",
        "Generator<i32>[]",
    ] {
        let source = main_with(&format!("function f(x: {form} | null): void {{}}"));
        let diagnostics = check_one(&source).expect_err(form);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S011, "{form}");
        assert_eq!(
            diagnostics[0].message,
            format!(
                "unions are limited to `Ref | null`; `{form} | null` is not a reference type union"
            )
        );
        assert_eq!(
            diagnostics[0].divergence,
            Some(Divergence::NullableNonReference)
        );
    }
    // Control: the same form with an uncounted element is accepted.
    assert!(check_one(&main_with(
        "class P { x: i32 = 1; }\nfunction f(x: P[] | null): void {}"
    ))
    .is_ok());
}

#[test]
fn rule_2_reads_the_instance_of_a_type_parameter() {
    let template = "function f<T>(x: T[] | null): i32 { return x?.length ?? -1; }\n\
                    export function main(): void { print(`${f<ELEMENT>(null)}`); }\n";
    let diagnostics =
        check_one(&template.replace("ELEMENT", "Promise<void>")).expect_err("a counted instance");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(
        diagnostics[0].message,
        "unions are limited to `Ref | null`; `Promise<void>[] | null` is not a reference type union"
    );
    // Control: an uncounted instance of the same template is accepted.
    assert!(check_one(&template.replace("ELEMENT", "i32")).is_ok());

    let boxed = "class Box<T> { v: T; constructor(v: T) { this.v = v; } }\n\
                 export function main(): void { const b = new Box<ELEMENT | null>(null); }\n";
    let diagnostics = check_one(&boxed.replace("ELEMENT", "Promise<void>[]"))
        .expect_err("a counted type argument");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert!(check_one(&boxed.replace("ELEMENT", "i32[]")).is_ok());
}

#[test]
fn rule_3_rejects_each_mirror_declaration() {
    let header = "// @subscript-c-header include=\"x.h\"\n";
    for (declaration, name, ty) in [
        (
            "declare function f(a: i32[] | null): void;",
            "f.a",
            "i32[] | null",
        ),
        ("declare function f(): i32[] | null;", "f", "i32[] | null"),
        (
            "declare class C { a: i32[] | null; constructor(a: i32[] | null); }",
            "C.a",
            "i32[] | null",
        ),
        (
            "declare class C { x: i32; constructor(a: i32[] | null); }",
            "C.constructor.a",
            "i32[] | null",
        ),
        (
            "declare class C { x: i32; constructor(x: i32); m(a: i32[] | null): void; }",
            "C.m.a",
            "i32[] | null",
        ),
        (
            "// @subscript-c-callback typedef=\"Cb\"\ntype Cb = (a: i32[] | null) => void;",
            "Cb",
            "(i32[] | null) => void",
        ),
        ("type A = i32[] | null;", "A", "i32[] | null"),
        (
            "declare function f(a: FixedArray<i32[] | null, 2>): void;",
            "f.a",
            "FixedArray<i32[] | null, 2>",
        ),
    ] {
        let diagnostics = mirror(&format!("{header}{declaration}")).expect_err(declaration);
        let expected = format!(
            "mirror `mirror.d.ts` declaration `{name}` has `{ty}`; the C ABI gives a nullable array no layout"
        );
        let found = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.message == expected)
            .unwrap_or_else(|| panic!("{declaration}: {diagnostics:?}"));
        assert_eq!(found.code, RuleCode::S100, "{declaration}");
    }
    // Control: a nullable boundary pointer in the same position is accepted.
    assert!(mirror(&format!(
        "{header}declare class C {{ x: i32; constructor(x: i32); }}\n\
         declare function f(a: C | null): void;"
    ))
    .is_ok());
}

#[test]
fn rule_4_json_reads_the_array_element() {
    let stringify = "class D { a: ELEMENT[] | null = null; }\n\
                     export function main(): void { print(JSON.stringify(new D())); }\n";
    assert!(check_one(&stringify.replace("ELEMENT", "i32")).is_ok());
    let diagnostics = check_one(&stringify.replace("ELEMENT", "RegExp"))
        .expect_err("the element is outside the JSON types");
    assert_eq!(diagnostics[0].code, RuleCode::S014, "{diagnostics:?}");
}

#[test]
fn rule_4_a_worker_message_field_stays_rejected() {
    let diagnostics = check_one(
        "class M { a: i32[] | null = null; }\n\
         function echo(inbox: Inbox<M>, outbox: Outbox<M>): void {}\n\
         export function main(): void { const w: Worker<M, M> = Worker.spawn(echo); }\n",
    )
    .expect_err("a nullable array is not transferable");
    assert_eq!(
        diagnostics[0].message,
        "message class `M` is not transferable: innermost field `M.a` has non-transferable type `i32[] | null`"
    );
}

#[test]
fn rule_5_an_unnarrowed_read_stays_rejected() {
    let body = |statement: &str| {
        format!("function f(a: i32[] | null): void {{ {statement} }}\nexport function main(): void {{}}\n")
    };
    for statement in ["const n = a.length;", "const n = a[0];", "a.push(1);"] {
        let diagnostics = check_one(&body(statement)).expect_err(statement);
        assert_eq!(diagnostics[0].code, RuleCode::S011, "{statement}");
        assert_eq!(
            diagnostics[0].message,
            "`i32[] | null` may be null here; narrow with a null check first"
        );
    }
    let diagnostics = check_one(&body("for (const x of a) {}")).expect_err("for of");
    assert_eq!(diagnostics[0].code, RuleCode::S014);
    // Control: the same read after a null test is accepted.
    assert!(check_one(&body("if (a !== null) { const n = a.length; }")).is_ok());
}

#[test]
fn rule_6_only_a_null_test_compares_a_nullable_array() {
    let body = |test: &str| {
        format!(
            "function f(a: i32[] | null, b: i32[] | null): boolean {{ return {test}; }}\n\
             export function main(): void {{}}\n"
        )
    };
    let diagnostics = check_one(&body("a === b")).expect_err("two arrays");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(check_one(&body("a === null")).is_ok());
}

#[test]
fn rule_7_a_nullable_string_stays_rejected() {
    let diagnostics = check_one(&main_with("function f(x: string | null): void {}"))
        .expect_err("a nullable string");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(
        diagnostics[0].message,
        "unions are limited to `Ref | null`; `string | null` is not a reference type union"
    );
}
