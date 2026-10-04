//! Rejected forms and accepted controls for compiler.md §155.
//! Cost: these tests use in-process checker calls.

use crate::{check_program, divergence::Divergence, Diagnostic, SourceFile};

fn accept(source: &str) {
    check_program(&[SourceFile::entry("main.ts", source)]).expect("accepted control");
}

fn reject(source: &str, message: &str) -> Vec<Diagnostic> {
    let diagnostics =
        check_program(&[SourceFile::entry("main.ts", source)]).expect_err("rejected form");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains(message), "{diagnostics:?}");
    diagnostics
}

#[test]
fn restricted_members_have_public_controls() {
    for modifier in ["private", "protected"] {
        for (declaration, usage) in [
            ("x: i32 = 1;", "new A().x;"),
            ("f(): i32 { return 1; }", "new A().f();"),
            ("static n: i32 = 1;", "A.n = 2;"),
            ("static f(): i32 { return 1; }", "A.f();"),
            ("get x(): i32 { return 1; }", "new A().x;"),
        ] {
            let source = format!(
                "class A {{ {modifier} {declaration} }} export function main(): void {{ {usage} }}"
            );
            assert!(reject(&source, modifier)[0].divergence.is_none());
            accept(&source.replace(modifier, "public"));
        }
        let source = format!("class A {{ {modifier} get x(): i32 {{ return 1; }} {modifier} set x(value: i32) {{}} }} export function main(): void {{ new A().x = 2; }}");
        reject(&source, modifier);
        accept(&source.replace(modifier, "public"));
    }
}

#[test]
fn lexical_access_in_methods_arrows_statics_and_initializers() {
    accept(include_str!(
        "../../../corpus/accept/a328-member-modifiers.ts"
    ));
    accept("class A { private x: i32 = 1; protected b: i32 = 2; private static n: i32 = 3; static total: i32 = A.n; read(other: A): i32 { const receiver: A = other; const outer: () => i32 = (): i32 => { const inner: () => i32 = (): i32 => receiver.x + receiver.b; return inner(); }; return outer(); } } export function main(): void {}");
    reject("class A { private x: i32 = 1; } class B { read(other: A): i32 { return other.x; } } export function main(): void {}", "private");
}

#[test]
fn constructor_access_has_public_and_lexical_controls() {
    for modifier in ["private", "protected"] {
        let source = format!("class A {{ {modifier} constructor() {{}} }} export function main(): void {{ new A(); }}");
        assert!(reject(&source, modifier)[0].divergence.is_none());
        accept(&source.replace(modifier, "public"));
        accept(&format!("class A {{ {modifier} constructor() {{}} static create(): A {{ const f: () => A = (): A => new A(); return f(); }} }} export function main(): void {{ A.create(); }}"));
    }
}

#[test]
fn abstract_construction_has_concrete_and_type_controls() {
    for source in [
        "abstract class A { x: i32 = 1; } export function main(): void { new A(); }",
        "abstract class A { static make(): void { new A(); } } export function main(): void {}",
    ] {
        assert!(
            reject(source, "class `A` is abstract; a `new` of it is rejected")[0]
                .divergence
                .is_none()
        );
        accept(&source.replace("abstract ", ""));
    }
    accept("abstract class A { static x: i32 = 1; } function use(value: A): void {} export function main(): void { print(`${A.x}`); }");
}

#[test]
fn every_readonly_write_form_has_a_mutable_control() {
    for write in ["a.x = 2;", "a.x += 1;", "a.x -= 1;", "a.x++;", "--a.x;"] {
        let source = format!("class A {{ readonly x: i32 = 1; }} export function main(): void {{ const a: A = new A(); {write} }}");
        assert!(reject(&source, "readonly")[0].divergence.is_none());
        accept(&source.replace("readonly ", ""));
    }
    for member in [
        "write(): void { this.x = 3; }",
        "constructor(other: A) { other.x = 3; }",
        "constructor() { const f: (other: A) => void = (other: A): void => { other.x = 3; }; }",
        "static write(other: A): void { other.x = 3; }",
    ] {
        let source = format!(
            "class A {{ readonly x: i32 = 1; {member} }} export function main(): void {{}}"
        );
        reject(&source, "readonly");
        accept(&source.replace("readonly ", ""));
    }
    reject("class A { readonly x: i32 = 1; constructor() { const f: () => void = (): void => { this.x = 3; }; } } export function main(): void {}", "readonly");
    accept("class A { readonly x: i32; readonly y: i32 = 8; constructor() { this.x = 1; this.x += 1; this.x++; this.x--; this.y = 2; this.y += 1; this.y++; } } export function main(): void { new A(); }");
}

#[test]
fn optional_function_types_poison_calls_without_follow_ons() {
    for call in ["f(1);", "f(1, 2);", ""] {
        let source = format!("function apply(f: (a: i32, b?: i32) => void): void {{ {call} }} export function main(): void {{ apply((a: i32, b: i32): void => {{}}); }}");
        let diagnostics = reject(&source, "optional parameters in function types");
        assert_eq!(diagnostics[0].code.as_str(), "S012");
        assert_eq!(
            diagnostics[0].divergence,
            Some(Divergence::OptionalParameter)
        );
        assert_eq!(diagnostics[0].pos.line, 1);
        accept(&source.replace("b?:", "b:").replace("f(1);", "f(1, 2);"));
    }
    for source in [
        "class A { cb: (a: i32, b?: i32) => void; constructor() { this.cb = (a: i32, b: i32): void => {}; } run(): void { this.cb(1); } } export function main(): void {}",
        "class A { g: (a: i32, b?: i32) => void = (a: i32, b: i32): void => {}; } export function main(): void { const a: A = new A(); a.g(1); }",
    ] {
        let diagnostics = reject(source, "optional parameters in function types");
        assert_eq!(diagnostics[0].code.as_str(), "S012");
        assert_eq!(diagnostics[0].divergence, Some(Divergence::OptionalParameter));
        accept(&source.replace("b?:", "b:").replace("(1);", "(1, 2);"));
    }
    reject("export function main(): void { const f: (a?: i32) => void = (a: i32): void => {}; f(); f(1); }", "optional parameters in function types");
}

#[test]
fn unsupported_nested_functions_keep_the_declaration_rejection() {
    for write in ["other.x = 3;", "print(`${other.x}`);"] {
        let source = format!("class A {{ private readonly x: i32 = 1; constructor(other: A) {{ function nested(): void {{ {write} }} nested(); }} }} export function main(): void {{}}");
        reject(&source, "nested declarations");
    }
}

#[test]
fn accessor_visibility_depends_on_read_or_write() {
    for modifier in ["private", "protected"] {
        for static_word in ["", "static "] {
            let receiver = if static_word.is_empty() {
                "new A()"
            } else {
                "A"
            };
            let prefix = format!("class A {{ public {static_word}get x(): i32 {{ return 1; }} {modifier} {static_word}set x(value: i32) {{}} }}");
            accept(&format!(
                "{prefix} export function main(): void {{ {receiver}.x; }}"
            ));
            reject(
                &format!("{prefix} export function main(): void {{ {receiver}.x = 2; }}"),
                modifier,
            );
        }
    }
}

#[test]
fn generic_instances_share_the_declaring_class_body() {
    accept("class A<T> { private x: i32 = 1; read(other: A<string>): i32 { return other.x; } } export function main(): void { new A<i32>().read(new A<string>()); }");
}

#[test]
fn direct_constructor_parameter_defaults_match_typescript() {
    accept("class A { readonly x: i32 = 1; constructor(value: i32 = this.x = 2) {} } export function main(): void { new A(); }");
}

#[test]
fn value_type_constructor_visibility_rejects_at_the_decorator() {
    for modifier in ["private", "protected"] {
        for decorator in ["@ValueType", "@ValueType({ align: 8 })"] {
            let source = format!("{decorator} class V {{ x: i32 = 0; {modifier} constructor() {{}} static make(): V {{ return new V(); }} }} export function main(): void {{ V.make(); }}");
            let diagnostics = reject(&source, &format!("rejects a {modifier} constructor"));
            assert_eq!(diagnostics[0].code.as_str(), "S100");
            assert!(diagnostics[0].divergence.is_none());
            assert_eq!(diagnostics[0].pos.col, 1);
            accept(&source.replace(modifier, "public"));
        }
    }
}

#[test]
fn readonly_assignment_guards_precede_unsupported_write_forms() {
    reject("class A { readonly x: i32 = 1; constructor() { (this).x = 2; } } export function main(): void {}", "readonly");
    let diagnostics = reject("class A { readonly x: i32 = 1; static write(): void { this.x = 2; } } export function main(): void {}", "static class receiver");
    assert!(diagnostics[0].divergence.is_none());
    for write in [
        "[a.x, b] = [3, 4];",
        "({ x: a.x } = { x: 3 });",
        "[a.x = 2] = [3];",
    ] {
        let source = format!("class A {{ readonly x: i32 = 1; }} export function main(): void {{ const a: A = new A(); let b: i32 = 0; {write} }}");
        assert!(reject(&source, "readonly")[0].divergence.is_none());
        assert_eq!(
            reject(&source.replace("readonly ", ""), "destructuring assignment")[0].divergence,
            Some(Divergence::AssignmentPattern)
        );
    }
    let source = "class N {} class A { readonly s: N | null = null; } export function main(): void { const a: A = new A(); a.s ??= new N(); }";
    assert!(reject(source, "readonly")[0].divergence.is_none());
    reject(&source.replace("readonly ", ""), "assignment operator");
    reject("class A { readonly x: i32 = 1; constructor() { this.x **= 2; } } export function main(): void {}", "assignment operator");
}
