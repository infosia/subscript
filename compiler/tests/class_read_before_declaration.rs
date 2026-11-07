//! A class read before its declaration is rejected (compiler.md §241).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

/// The declarations that each case reads, after the case in the source.
const DECLARATIONS: &str = r#"
class B {
  n: i32 = 1;
  static k(x: i32): i32 { return x; }
  static m(x: i32): void {}
}
enum E { A, B }
class Box<T> {
  v: T;
  constructor(v: T) { this.v = v; }
}
"#;

fn check(case: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!(
        "type CB = (x: i32) => void;\n{case}\n{DECLARATIONS}\nexport function main(): void {{}}\n"
    );
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

/// The name of the one diagnostic of a rejected case: the class or enum
/// that the case reads.
fn rejected_name(case: &str) -> String {
    let errors = check(case).expect_err(case);
    assert_eq!(errors.len(), 1, "{case}: {errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100, "{case}: {errors:?}");
    errors[0]
        .message
        .strip_suffix("` is used before its declaration")
        .and_then(|message| message.strip_prefix('`'))
        .unwrap_or_else(|| panic!("{case}: {}", errors[0].message))
        .to_string()
}

/// Rules 1 and 2: each form of the first table reads a class, a generic
/// class, or an enum before its declaration, evaluated at once.
#[test]
fn a_read_evaluated_at_once_is_rejected() {
    for (case, name) in [
        ("const early: CB = B.m;", "B"),
        ("const v: i32 = B.k(1);", "B"),
        ("const b: B = new B();", "B"),
        ("const gb: Box<i32> = new Box<i32>(1);", "Box"),
        ("const e: E = E.A;", "E"),
        ("class A { static x: i32 = B.k(3); }", "B"),
        ("if (true) { const u: i32 = B.k(6); }", "B"),
        ("for (let i: i32 = 0; i < B.k(0); i++) {}", "B"),
        ("const s: i32 = ((): i32 => B.k(7))();", "B"),
        // A lambda with a result annotation whose type the binding decides
        // first is called at once too.
        ("const s = ((): i32 => B.k(7))();", "B"),
    ] {
        assert_eq!(rejected_name(case), name, "{case}");
    }
}

/// Rule 2: each form of the second table is in a deferred body, so it
/// stays accepted. The firing control beside each case: the same read
/// evaluated at once in the same program is rejected.
#[test]
fn a_deferred_read_is_accepted() {
    for (case, control) in [
        (
            "const f1 = (): i32 => new B().n;",
            "const f1: i32 = new B().n;",
        ),
        (
            "function f2(): i32 { return B.k(1); }",
            "const f2: i32 = B.k(1);",
        ),
        (
            "class A { y: i32 = B.k(4); }",
            "class A { static y: i32 = B.k(4); }",
        ),
        (
            "class A { m(): i32 { return B.k(5); } }",
            "class A { static z: i32 = B.k(5); m(): i32 { return 1; } }",
        ),
        (
            "class A { constructor() { const c: i32 = B.k(8); } }",
            "const c: i32 = B.k(8);",
        ),
    ] {
        check(case).unwrap_or_else(|errors| panic!("{case}: {errors:?}"));
        assert_eq!(rejected_name(control), "B", "{control}");
    }
}

/// Rule 3: a read inside the declaration is not before its start. The
/// control: the same read before the declaration is rejected.
#[test]
fn a_read_inside_the_declaration_is_accepted() {
    let source = "class S { static a: i32 = 1; static b: i32 = S.a + 1; }\nexport function main(): void {}\n";
    check_program(&[SourceFile::new("test.ts", source)])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    let control = "const z: i32 = S.a;\nclass S { static a: i32 = 1; static b: i32 = S.a + 1; }\nexport function main(): void {}\n";
    // The C14 check of the static field read reports the same read too.
    let errors = check_program(&[SourceFile::new("test.ts", control)]).expect_err("the control");
    assert!(
        errors
            .iter()
            .any(|error| error.message == "`S` is used before its declaration"),
        "{errors:?}"
    );
}

/// Rule 2: a parameter default of a lambda runs when the lambda is called,
/// with or without a parameter annotation, at the top level and in a
/// block. The firing control beside each case: the same lambda called at
/// once is rejected.
#[test]
fn a_lambda_parameter_default_is_deferred() {
    for (case, control, name) in [
        (
            "const f = (x = B.k(1)): i32 => x;",
            "const f = ((x = B.k(1)): i32 => x)();",
            "B",
        ),
        (
            "const a = (x = 1, y = E.A): E => y;",
            "const a = ((x = 1, y = E.A): E => y)();",
            "E",
        ),
        (
            "if (true) { const g = (x = B.k(1)): i32 => x; }",
            "if (true) { const g = ((x = B.k(1)): i32 => x)(); }",
            "B",
        ),
        (
            "const h = (x: i32 = B.k(1)): i32 => x;",
            "const h = (): i32 => B.k(1);\nconst hv: i32 = ((): i32 => B.k(1))();",
            "B",
        ),
    ] {
        check(case).unwrap_or_else(|errors| panic!("{case}: {errors:?}"));
        assert_eq!(rejected_name(control), name, "{control}");
    }
}

/// Rule 4: a type reference is not a value reference. The control: a value
/// reference in the same place is rejected.
#[test]
fn a_type_reference_is_accepted() {
    check("const f: (b: B) => i32 = (b: B): i32 => b.n;")
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    assert_eq!(rejected_name("const g: i32 = new B().n;"), "B");
}

/// Rule 2: a parameter default of a function, a method, or a constructor,
/// a getter body, a setter body, a static method body, and a descriptor
/// field default run later. The firing control beside each case: the same
/// read evaluated at once, in a static field or a top-level initializer, is
/// rejected.
#[test]
fn a_deferred_member_read_is_accepted() {
    let in_static_field = "class A { static s: i32 = B.k(1); }";
    let at_top_level = "const c: i32 = B.k(1);";
    for (case, control, name) in [
        (
            "function fd(x = B.k(1)): i32 { return x; }",
            at_top_level,
            "B",
        ),
        (
            "function fd(x: i32 = B.k(1)): i32 { return x; }",
            at_top_level,
            "B",
        ),
        (
            "class A { m(x = B.k(1)): i32 { return x; } }",
            in_static_field,
            "B",
        ),
        (
            "class A { m(x: i32 = B.k(1)): i32 { return x; } }",
            in_static_field,
            "B",
        ),
        (
            "class A { constructor(x = B.k(1)) {} }",
            in_static_field,
            "B",
        ),
        (
            "class A { constructor(x: i32 = B.k(1)) {} }",
            in_static_field,
            "B",
        ),
        (
            "class A { get g(): i32 { return B.k(1); } }",
            in_static_field,
            "B",
        ),
        (
            "class A { get g(): i32 { return 1; } set g(v: i32) { const c: i32 = B.k(v); } }",
            in_static_field,
            "B",
        ),
        (
            "class A { static sm(): i32 { return B.k(1); } }",
            in_static_field,
            "B",
        ),
        (
            "@Descriptor class P { e?: E = E.A; }",
            "class A { static s: E = E.A; }",
            "E",
        ),
    ] {
        check(case).unwrap_or_else(|errors| panic!("{case}: {errors:?}"));
        assert_eq!(rejected_name(control), name, "{control}");
    }
}

/// Rule 1: a `const enum` has no value at run time, so a read before it
/// stays accepted. The control: the same read of a plain `enum` is
/// rejected.
#[test]
fn a_const_enum_read_before_its_declaration_is_accepted() {
    let program = |kind: &str| {
        format!(
            "const c: C = C.X;\n{kind} C {{ X, Y }}\nexport function main(): void {{ print(`${{c === C.X}}`); }}\n"
        )
    };
    let source = program("const enum");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    let control = program("enum");
    let errors =
        check_program(&[SourceFile::new("test.ts", control.as_str())]).expect_err("the control");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, "`C` is used before its declaration");
}
