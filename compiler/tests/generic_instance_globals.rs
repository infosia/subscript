//! Generic instance bodies use complete module signatures (§138).
//! Cost: checker calls only; no native program compiles.

use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn early_instance_reads_report_the_full_member_route() {
    for (body, initializer, early, member) in [
        (
            "x: T; constructor(x: T) { this.x = x; } peek(): i32 { return m.v; }",
            "new Box<i32>(1)",
            "const early: i32 = b.peek();",
            "peek",
        ),
        (
            "x: T; constructor(x: T) { this.x = x; print(`${m.v}`); }",
            "new Box<i32>(1)",
            "",
            "constructor",
        ),
        (
            "x: T; value: i32 = m.v; constructor(x: T) { this.x = x; }",
            "new Box<i32>(1)",
            "",
            "constructor",
        ),
        (
            "x: T; constructor(x: T) { this.x = x; } get value(): i32 { return m.v; }",
            "new Box<i32>(1)",
            "const early: i32 = b.value;",
            "value",
        ),
        (
            "x: T; constructor(x: T) { this.x = x; } get value(): i32 { return 0; } set value(v: i32) { print(`${m.v}`); }",
            "new Box<i32>(1)",
            "b.value = 1;",
            "value=",
        ),
    ] {
        let source = format!(
            "class Foo {{ v: i32 = 7; }} class Box<T> {{ {body} }} \
             const b: Box<i32> = {initializer}; {early} \
             const m: Foo = new Foo(); export function main(): void {{}}"
        );
        let errors = check_program(&[SourceFile::entry("main.ts", source)])
            .expect_err("the instance reads an uninitialized global");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].message,
            format!("`m` is accessed before its declaration, through `Box<i32>.{member}`")
        );
    }
}

#[test]
fn unresolved_instance_names_report_s016() {
    for body in [
        "peek(): i32 { return missing; }",
        "constructor() { print(`${missing}`); }",
        "value: i32 = missing;",
        "get value(): i32 { return missing; }",
    ] {
        let source = format!(
            "class Box<T> {{ {body} }} const b: Box<i32> = new Box<i32>(); \
             export function main(): void {{}}"
        );
        let errors = check_program(&[SourceFile::entry("main.ts", source)])
            .expect_err("the instance name does not resolve");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S016);
        assert_eq!(errors[0].message, "unknown name `missing`");
    }
}
