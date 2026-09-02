//! Method type parameters, instance and static (`compiler.md` §82.4).
//!
//! Each distinct type-argument list yields one method instance named
//! `m<A>`. The template never reaches the HIR.

use subscript_compiler::{
    check_program, hir, render_diagnostics, Diagnostic, RuleCode, SourceFile,
};

fn module(source: &str) -> hir::Module {
    check_program(&[SourceFile::new("test.ts", source)]).expect("the program must check")
}

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("test.ts", source)]).expect_err("the program must fail")
}

/// Every method name that a `Callee::Method` call in this body names.
fn called_method_names(body: &[hir::Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for statement in body {
        walk_stmt(statement, &mut names);
    }
    names
}

fn walk_stmt(statement: &hir::Stmt, names: &mut Vec<String>) {
    match statement {
        hir::Stmt::Let { init, .. } => walk_expr(init, names),
        hir::Stmt::Expr(expr) => walk_expr(expr, names),
        hir::Stmt::Return {
            value: Some(value), ..
        } => walk_expr(value, names),
        _ => {}
    }
}

fn walk_expr(expr: &hir::Expr, names: &mut Vec<String>) {
    if let hir::ExprKind::Call { callee, args } = &expr.kind {
        if let hir::Callee::Method { recv, name } = callee {
            names.push(name.clone());
            walk_expr(recv, names);
        }
        for arg in args {
            walk_expr(arg, names);
        }
    }
}

const TWO_CALLS: &str = "class Box {\n\
                         \x20 identity<T>(value: T): T { return value; }\n\
                         }\n\
                         export function main(): void {\n\
                         \x20 const box: Box = new Box();\n\
                         \x20 const first: i32 = box.identity<i32>(1);\n\
                         \x20 const second: i32 = box.identity<i32>(2);\n\
                         \x20 const third: string = box.identity<string>(\"x\");\n\
                         \x20 print(`${first}${second}${third}`);\n\
                         }\n";

#[test]
fn two_calls_at_one_type_list_yield_one_instance() {
    let module = module(TWO_CALLS);
    let names: Vec<&str> = module.classes[0]
        .methods
        .iter()
        .map(|method| method.name.as_str())
        .collect();
    assert_eq!(names, ["identity<i32>", "identity<string>"]);
}

#[test]
fn the_call_names_the_instance_not_the_template() {
    let module = module(TWO_CALLS);
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main must exist");
    assert_eq!(
        called_method_names(&main.body),
        ["identity<i32>", "identity<i32>", "identity<string>"]
    );
}

#[test]
fn a_static_instance_lowers_through_the_static_symbol() {
    let source = "class Box {\n\
                  \x20 v: i32;\n\
                  \x20 constructor(v: i32) { this.v = v; }\n\
                  \x20 static create<T>(values: T[]): Box { return new Box(values.length); }\n\
                  }\n\
                  export function main(): void {\n\
                  \x20 print(`${Box.create<i32>([1, 2]).v}`);\n\
                  }\n";
    let module = module(source);
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Box.create<i32>"),
        "the static instance must be a free function named by the static symbol: {:?}",
        module
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        module.classes[0].methods.is_empty(),
        "a static instance must not join the instance method table"
    );
}

#[test]
fn a_wrong_type_argument_count_fails() {
    let source = "class Box {\n\
                  \x20 identity<T>(value: T): T { return value; }\n\
                  }\n\
                  export function main(): void {\n\
                  \x20 const box: Box = new Box();\n\
                  \x20 print(`${box.identity<i32, u32>(1)}`);\n\
                  }\n";
    let diagnostics = diagnostics(source);
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "`identity` expects 1 type argument(s), got 2"
    );
    assert_eq!(diagnostics[0].pos.line, 6);
}

#[test]
fn a_bodiless_generic_method_fails_during_collection() {
    let source = "class C { m<T>(value: T): T; }\n\
                  export function main(): void {\n\
                  \x20 const c: C = new C();\n\
                  \x20 c.m<i32>(1);\n\
                  }\n";
    let files = [SourceFile::new("test.ts", source)];
    let diagnostics = check_program(&files).expect_err("the bodiless template must fail");
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].message, "function bodies are required");
    assert_eq!(diagnostics[0].pos.line, 1);
    assert!(
        !render_diagnostics(&files, &diagnostics).contains("= TypeScript accepts:"),
        "a plain class has no divergence block"
    );
}

#[test]
fn a_bodiless_generic_free_function_fails_during_collection() {
    let diagnostics = diagnostics("function f<T>(value: T): T;\n");
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].message, "function bodies are required");
    assert_eq!(diagnostics[0].pos.line, 1);
}

#[test]
fn duplicate_method_type_parameters_fail_during_collection() {
    let diagnostics = diagnostics(
        "class C { m<T, T>(value: T): T { return value; } }\n\
         export function main(): void {\n\
         \x20 const c: C = new C();\n\
         \x20 c.m<i32>(1);\n\
         }\n",
    );
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S017);
    assert_eq!(diagnostics[0].message, "duplicate type parameter `T`");
    assert_eq!(diagnostics[0].pos.line, 1);
}

#[test]
fn declare_class_generic_method_keeps_the_body_rejection() {
    let source = "declare class C { m<T>(value: T): T; }\n";
    let files = [SourceFile::new("test.ts", source)];
    let diagnostics = check_program(&files).expect_err("the declared template must fail");
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].message, "function bodies are required");
    assert!(
        render_diagnostics(&files, &diagnostics).contains("= TypeScript accepts:"),
        "the declared method must render its divergence block"
    );
}

#[test]
fn the_declared_name_owns_the_member_namespace() {
    let source = "class Box {\n\
                  \x20 identity: i32 = 1;\n\
                  \x20 identity<T>(value: T): T { return value; }\n\
                  }\n\
                  export function main(): void {}\n";
    let diagnostics = diagnostics(source);
    assert_eq!(diagnostics[0].code, RuleCode::S017);
    assert_eq!(diagnostics[0].pos.line, 3);
}

#[test]
fn a_value_class_receiver_carries_a_generic_method() {
    let source = "@CStruct\n\
                  class Vec2 {\n\
                  \x20 x: f32;\n\
                  \x20 y: f32;\n\
                  \x20 pick<T>(value: T): T { return value; }\n\
                  }\n\
                  export function main(): void {\n\
                  \x20 const point: Vec2 = new Vec2();\n\
                  \x20 print(`${point.pick<i32>(9)}${point.x}${point.y}`);\n\
                  }\n";
    let module = module(source);
    let names: Vec<&str> = module.classes[0]
        .methods
        .iter()
        .map(|method| method.name.as_str())
        .collect();
    assert_eq!(names, ["pick<i32>"]);
}

#[test]
fn a_generic_method_read_as_a_value_keeps_the_method_as_value_rule() {
    let source = "class Box {\n\
                  \x20 identity<T>(value: T): T { return value; }\n\
                  }\n\
                  export function main(): void {\n\
                  \x20 const box: Box = new Box();\n\
                  \x20 const f: (v: i32) => i32 = box.identity;\n\
                  \x20 print(`${f(1)}`);\n\
                  }\n";
    let diagnostics = diagnostics(source);
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "method `identity` may only be called, not read as a value"
    );
    assert_eq!(diagnostics[0].pos.line, 6);
}
