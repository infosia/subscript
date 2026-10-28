//! A private name is a private member (compiler.md §206).
//!
//! Cost: twenty-one checker calls on one-file or two-file programs, with no
//! code generation and no external process.

use super::*;
use crate::divergence::Divergence;

fn check(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::entry("main.ts", src)])
}

fn only(src: &str) -> Diagnostic {
    let diagnostics = check(src).expect_err(src);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    diagnostics.into_iter().next().unwrap()
}

/// Rules 1, 3, and 4: `#x` and `x` are two fields, in declaration order.
#[test]
fn a_private_name_and_its_plain_name_are_two_fields() {
    let module = check(
        "class P { a: i32 = 0; #x: i32 = 1; x: i32 = 2; }\n\
         export function main(): void { print(`${new P().x}`); }\n",
    )
    .unwrap();
    let class = module
        .classes
        .iter()
        .find(|class| class.name == "P")
        .unwrap();
    let names: Vec<&str> = class.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["a", "#x", "x"]);
}

/// Rule 2: code outside the class body gets the `private` text.
#[test]
fn a_private_name_outside_its_class_is_private() {
    let diagnostic = only(
        "class C { #x: i32 = 1; static #s: i32 = 2; }\n\
         export function main(): void { print(`${new C().#x}`); }\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(diagnostic.message, "member `#x` is private in class `C`");
    assert_eq!((diagnostic.pos.line, diagnostic.pos.col), (2, 49));
    // Control: the same read inside the class body is accepted.
    assert!(check(
        "class C { #x: i32 = 1; f(o: C): i32 { return o.#x; } }\n\
         export function main(): void { print(`${new C().f(new C())}`); }\n",
    )
    .is_ok());
}

const PARSE: &str = "`JSON.parse` cannot set the private name `#x` of `Inner`";

/// Rule 5: `JSON.parse<C>` fails if `C` reaches a `#x` field.
#[test]
fn json_parse_cannot_set_a_private_name() {
    for target in ["Inner", "Outer", "Inner[]", "Outer | null"] {
        let diagnostic = only(&format!(
            "class Inner {{ #x: i32 = 1; y: i32 = 2; }}\n\
             class Outer {{ inner: Inner = new Inner(); }}\n\
             export function main(): void {{ JSON.parse<{target}>(\"null\"); }}\n"
        ));
        assert_eq!(diagnostic.code, RuleCode::S100, "{target}");
        assert_eq!(diagnostic.message, PARSE, "{target}");
        assert_eq!(
            diagnostic.divergence,
            Some(Divergence::JsonParsePrivateName),
            "{target}"
        );
    }
    // Control: the same class with a `private` field parses.
    assert!(check(
        "class Inner { private x: i32 = 1; y: i32 = 2; }\n\
         export function main(): void { JSON.parse<Inner>(\"null\"); }\n",
    )
    .is_ok());
}

/// Rule 6: a descriptor class and a mirror class keep each rejection.
#[test]
fn a_descriptor_or_mirror_private_name_is_rejected() {
    let diagnostic = only("@Descriptor\nclass D { #x!: i32; }\nexport function main(): void {}\n");
    assert_eq!(
        diagnostic.message,
        "descriptor classes cannot declare the private name `#x`"
    );
    assert_eq!(
        diagnostic.divergence,
        Some(Divergence::DescriptorPrivateName)
    );
    // Control: a public descriptor member is accepted.
    assert!(check("@Descriptor\nclass D { x!: i32; }\nexport function main(): void {}\n").is_ok());

    let mirror = |member: &str| {
        check_program(&[
            SourceFile::ambient("mirror.d.ts", format!("declare class M {{ {member} }}\n")),
            SourceFile::entry("main.ts", "export function main(): void {}\n"),
        ])
    };
    let diagnostics = mirror("#x: i32;").expect_err("mirror private field");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "mirror classes cannot declare the private name `#x`"
    );
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::MirrorPrivateName)
    );
    // Control: a public mirror field is accepted.
    assert!(mirror("x: i32;").is_ok());
}

/// Rule 6: `o?.#x` and `#x in o` stay rejected.
#[test]
fn an_optional_chain_and_an_in_operand_keep_their_rejection() {
    let diagnostic = only(
        "class C { #x: i32 = 1; f(o: C | null): void { o?.#x; } }\n\
         export function main(): void {}\n",
    );
    assert_eq!(
        diagnostic.message,
        "an optional chain cannot contain a private name"
    );
    let diagnostic = only(
        "class C { #x: i32 = 1; f(o: C): boolean { return #x in o; } }\n\
         export function main(): void {}\n",
    );
    assert_eq!(diagnostic.message, "operator outside the decided surface");
}

/// Rule 1: a static and an instance member cannot share a private name.
#[test]
fn a_static_and_an_instance_member_cannot_share_a_private_name() {
    for (members, name, column) in [
        ("static #x: i32 = 1; #x: i32 = 2;", "#x", 31),
        ("#x: i32 = 2; static #x: i32 = 1;", "#x", 31),
        (
            "get #v(): i32 { return 1; } static get #v(): i32 { return 2; }",
            "#v",
            50,
        ),
        ("static #f(): void {} #f(): void {}", "#f", 32),
    ] {
        let diagnostic = only(&format!(
            "class A {{ {members} }}\nexport function main(): void {{}}\n"
        ));
        assert_eq!(diagnostic.code, RuleCode::S100, "{members}");
        assert_eq!(
            diagnostic.message,
            format!("static and instance members cannot share the private name `{name}`"),
            "{members}"
        );
        assert_eq!(
            (diagnostic.pos.line, diagnostic.pos.col),
            (1, column),
            "{members}"
        );
        assert_eq!(diagnostic.divergence, None, "{members}");
    }
    // Control: a static and an instance member can share a plain name.
    assert!(
        check("class A { static x: i32 = 1; x: i32 = 2; }\nexport function main(): void {}\n")
            .is_ok()
    );
}

/// Rule 5: a private name field outside the JSON types blocks
/// `JSON.stringify`, and the text names it.
#[test]
fn json_stringify_names_the_private_field_that_blocks_it() {
    let diagnostic = only(
        "class Holder { #m: Map<string, i32> = new Map<string, i32>(); y: i32 = 1; }\n\
         export function main(): void { print(JSON.stringify([new Holder()])); }\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S014);
    assert_eq!(
        diagnostic.message,
        "`JSON.stringify` cannot serialize `Holder[]`; the private field `#m` has type \
         `Map<string, i32>`, which `JSON.stringify` cannot serialize"
    );
    // Control: a public field outside the JSON types keeps the domain text.
    let diagnostic = only(
        "class Holder { m: Map<string, i32> = new Map<string, i32>(); }\n\
         export function main(): void { print(JSON.stringify(new Holder())); }\n",
    );
    assert!(
        diagnostic.message.ends_with("classes, and Ref | null"),
        "{}",
        diagnostic.message
    );
}
