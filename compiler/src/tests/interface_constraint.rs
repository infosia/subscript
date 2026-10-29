//! An interface is a constraint (compiler.md §208).
//!
//! Cost: 24 checker calls on one-file programs, with no code
//! generation and no external process; about 0.02 s in a debug build.

use super::*;
use crate::divergence::Divergence;

fn check(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::entry("main.ts", src)])
}

/// The debug text of `value` with every source position removed.
fn without_positions(value: &impl std::fmt::Debug) -> String {
    let text = format!("{value:?}");
    let mut result = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("Pos { file:") {
        result.push_str(&rest[..start]);
        let end = rest[start..]
            .find('}')
            .map_or(rest.len(), |end| start + end + 1);
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

/// Clears the written-annotation flag of each local read (§162 rule 3a).
/// The flag records the declaration of the parameter, which names `T` in
/// the template and `C` in the direct function; the call does not read it.
fn clear_annotation_flags(statement: &mut hir::Stmt) {
    fn expression(value: &mut hir::Expr) {
        if let hir::ExprKind::Local(_, _, annotated) = &mut value.kind {
            *annotated = false;
        }
        for child in value.children_mut() {
            match child {
                hir::HirChildMut::Expr(inner) => expression(inner),
                hir::HirChildMut::Stmt(inner) => clear_annotation_flags(inner),
            }
        }
    }
    for child in statement.children_mut() {
        match child {
            hir::HirChildMut::Expr(inner) => expression(inner),
            hir::HirChildMut::Stmt(inner) => clear_annotation_flags(inner),
        }
    }
}

/// The body of `function` with no positions and no annotation flags.
fn call_shape(function: &hir::Function) -> String {
    let mut body = function.body.clone();
    body.iter_mut().for_each(clear_annotation_flags);
    without_positions(&body)
}

fn function<'m>(module: &'m hir::Module, name: &str) -> &'m hir::Function {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| {
            let names: Vec<_> = module.functions.iter().map(|f| &f.name).collect();
            panic!("no function `{name}` in {names:?}")
        })
}

#[test]
fn a_call_through_a_constraint_is_the_direct_call_in_each_instance() {
    let module = check(
        "interface I { m(a: i32): i32; get g(): i32; }\n\
         class C implements I {\n\
         \x20 m(a: i32): i32 { return a + 1; }\n\
         \x20 get g(): i32 { return 5; }\n\
         }\n\
         function viaConstraint<T extends I>(x: T): i32 { return x.m(2) + x.g; }\n\
         function direct(x: C): i32 { return x.m(2) + x.g; }\n\
         export function main(): void { print(`${viaConstraint(new C()) + direct(new C())}`); }\n",
    )
    .expect("the program checks");
    let instance = function(&module, "viaConstraint<C>");
    let direct = function(&module, "direct");
    assert_eq!(instance.params.len(), 1);
    assert_eq!(instance.params[0].ty, direct.params[0].ty);
    assert_eq!(call_shape(instance), call_shape(direct));
    // Control: the comparison sees a different receiver member.
    let other = check(
        "class C { m(a: i32): i32 { return a + 1; } n(a: i32): i32 { return a; } get g(): i32 { return 5; } }\n\
         function direct(x: C): i32 { return x.m(2) + x.g; }\n\
         function other(x: C): i32 { return x.n(2) + x.g; }\n\
         export function main(): void { print(`${direct(new C()) + other(new C())}`); }\n",
    )
    .expect("the control checks");
    assert_ne!(
        call_shape(function(&other, "direct")),
        call_shape(function(&other, "other"))
    );
}

const MISSING: &str = "class `C` does not implement `m` of interface `I`";

/// Each case is a class body that does not implement `m` of `I`, the
/// expected message and divergence, and the control body that does.
const RULE_3_CASES: &[(&str, &str, &str, Option<Divergence>, &str)] = &[
    (
        "parameter type",
        "m(a: i64): i32 { return 1; }",
        "class `C` implements `m` of interface `I` with type `(i64) => i32`; the interface requires `(i32) => i32`",
        Some(Divergence::InterfaceMemberType),
        "m(a: i32): i32 { return 1; }",
    ),
    (
        "result type",
        "m(a: i32): i64 { return 1; }",
        "class `C` implements `m` of interface `I` with type `(i32) => i64`; the interface requires `(i32) => i32`",
        Some(Divergence::InterfaceMemberType),
        "m(a: i32): i32 { return 1; }",
    ),
    (
        "parameter type that tsc also rejects",
        "m(a: string): i32 { return 1; }",
        "class `C` implements `m` of interface `I` with type `(string) => i32`; the interface requires `(i32) => i32`",
        Some(Divergence::InterfaceMemberType),
        "m(a: i32): i32 { return 1; }",
    ),
    (
        "private member",
        "private m(a: i32): i32 { return 1; }",
        MISSING,
        None,
        "public m(a: i32): i32 { return 1; }",
    ),
    (
        "static member",
        "static m(a: i32): i32 { return 1; }",
        MISSING,
        None,
        "m(a: i32): i32 { return 1; }",
    ),
];

fn rule_3_program(body: &str) -> String {
    format!(
        "interface I {{ m(a: i32): i32; }}\n\
         class C implements I {{ {body} }}\n\
         export function main(): void {{}}\n"
    )
}

#[test]
fn a_class_implements_each_member_with_its_exact_types() {
    for (name, body, message, divergence, control) in RULE_3_CASES {
        let diagnostics = check(&rule_3_program(body)).expect_err(name);
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{name}");
        assert_eq!(diagnostics[0].message, *message, "{name}");
        assert_eq!(
            (diagnostics[0].pos.line, diagnostics[0].pos.col),
            (2, 7),
            "{name}"
        );
        assert_eq!(diagnostics[0].divergence, *divergence, "{name}");
        assert!(
            check(&rule_3_program(control)).is_ok(),
            "{name}: control {control}"
        );
    }
}

#[test]
fn an_instance_field_implements_a_read_accessor() {
    let program = |field: &str| {
        format!(
            "interface I {{ get g(): i32; }}\n\
             class C implements I {{ {field} }}\n\
             function read<T extends I>(x: T): i32 {{ return x.g; }}\n\
             export function main(): void {{ print(`${{read(new C())}}`); }}\n"
        )
    };
    assert!(check(&program("g: i32 = 4;")).is_ok());
    let diagnostics = check(&program("g: i64 = 4;")).expect_err("a field of another type");
    assert_eq!(
        diagnostics[0].message,
        "class `C` implements `g` of interface `I` with type `i64`; the interface requires `i32`"
    );
}

#[test]
fn an_async_method_implements_a_promise_member() {
    let program = |method: &str| {
        format!(
            "interface I {{ run(n: i32): Promise<i32>; }}\n\
             class C implements I {{ {method} }}\n\
             async function go<T extends I>(x: T): Promise<i32> {{ return await x.run(2); }}\n\
             export async function main(): Promise<void> {{ print(`${{await go(new C())}}`); }}\n"
        )
    };
    check(&program("async run(n: i32): Promise<i32> { return n; }"))
        .expect("an async method implements the member");
    // Control: a synchronous method with result `i32` does not.
    let diagnostics =
        check(&program("run(n: i32): i32 { return n; }")).expect_err("a synchronous method");
    assert_eq!(
        diagnostics[0].message,
        "class `C` implements `run` of interface `I` with type `(i32) => i32`; \
         the interface requires `(i32) => Promise<i32>`"
    );
}

#[test]
fn a_member_that_failed_its_own_check_reports_once() {
    let program = |method: &str| {
        format!(
            "interface I {{ m(): i32; }}\n\
             class C implements I {{ {method} }}\n\
             export function main(): void {{}}\n"
        )
    };
    for (name, method) in [
        ("unknown result type", "m(): Missing { return 1; }"),
        ("generator method", "*m(): Generator<i32> { yield 1; }"),
    ] {
        let diagnostics = check(&program(method)).expect_err(name);
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:?}");
        assert!(
            !diagnostics[0].message.contains("interface `I`"),
            "{name}: {diagnostics:?}"
        );
    }
    // Control: a member that the class does not declare reports.
    let diagnostics = check(&program("n(): i32 { return 1; }")).expect_err("missing");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "class `C` does not implement `m` of interface `I`"
    );
}

const RULE_4_PRELUDE: &str = "interface I { f(): i32; }\n\
     class C implements I { f(): i32 { return 1; } }\n\
     class Z { f(): i32 { return 2; } }\n\
     function g<T extends I>(x: T): i32 { return x.f(); }\n";

#[test]
fn a_constrained_type_parameter_satisfies_an_interface_constraint() {
    for (name, function) in [
        (
            "constraint I",
            "function h<U extends I>(x: U): i32 { return g(x); }",
        ),
        (
            "constraint C",
            "function h<U extends C>(x: U): i32 { return g(x); }",
        ),
    ] {
        let source = format!(
            "{RULE_4_PRELUDE}{function}\nexport function main(): void {{ print(`${{h(new C())}}`); }}\n"
        );
        check(&source).unwrap_or_else(|diagnostics| panic!("{name}: {diagnostics:?}"));
    }
    // Control: a class constraint that does not declare `implements I`.
    let source = format!(
        "{RULE_4_PRELUDE}function h<U extends Z>(x: U): i32 {{ return g(x); }}\n\
         export function main(): void {{}}\n"
    );
    let diagnostics = check(&source).expect_err("constraint Z");
    assert_eq!(
        diagnostics[0].message,
        "`U` does not declare `implements I`"
    );
}

#[test]
fn a_type_parameter_argument_has_no_typescript_block() {
    let source = |argument: &str| {
        format!(
            "{RULE_4_PRELUDE}function h<U>(x: U): i32 {{ return 0; }}\n\
             function k<V>(x: V): i32 {{ return g({argument}); }}\n\
             export function main(): void {{}}\n"
        )
    };
    let diagnostics = check(&source("x")).expect_err("type parameter argument");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "`V` does not declare `implements I`"
    );
    assert_eq!(diagnostics[0].divergence, None);
    // Control: a class argument carries the block.
    let diagnostics = check(&source("new Z()")).expect_err("class argument");
    assert_eq!(
        diagnostics[0].message,
        "`Z` does not declare `implements I`"
    );
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::InterfaceConstraintUndeclared)
    );
}
