//! A recursive `void` lambda decides its type from its syntax
//! (compiler.md §239).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const CYCLE: &str = "initializer type depends on its own undecided type";

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source = format!("{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
}

fn accepts(body: &str) -> hir::Module {
    check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"))
}

/// The type of the module binding `name`.
fn global_type(module: &hir::Module, name: &str) -> Type {
    module
        .globals
        .iter()
        .find(|global| global.name == name)
        .map(|global| global.ty.clone())
        .expect("the binding")
}

/// Rule 1: a block body with no `return` operand decides the result
/// `void` from its syntax, so the body can call the binding that holds
/// the lambda, directly or through another binding. An async lambda has
/// the result `Promise<void>`.
#[test]
fn a_recursive_void_block_lambda_is_accepted() {
    let void_function = Type::func(vec![Type::I32], Type::Void);
    let module = accepts("const rec = (n: i32) => { print(`r${n}`); if (n > 0) rec(n - 1); };");
    assert_eq!(global_type(&module, "rec"), void_function);
    let module = accepts(
        "const ping = (n: i32) => { if (n > 0) pong(n - 1); };\nconst pong = (n: i32) => { print(`p${n}`); ping(n); };",
    );
    assert_eq!(global_type(&module, "ping"), void_function);
    assert_eq!(global_type(&module, "pong"), void_function);
    let module = accepts("const arec = async (n: i32) => { if (n > 0) { await arec(n - 1); } };");
    assert_eq!(
        global_type(&module, "arec"),
        Type::func(vec![Type::I32], Type::async_handle(Type::Void))
    );
}

/// Rule 2: an expression body decides the result from its type, so a
/// recursive expression body stays a cycle. The control: a block body of
/// the same recursion is accepted (rule 1), and a non-recursive expression
/// body keeps the type of its body.
#[test]
fn an_expression_body_decides_its_result_from_its_type() {
    let errors = check("const e = (n: i32) => n > 0 ? e(n - 1) : 0;").expect_err("cycle");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, CYCLE);
    accepts("const e = (n: i32) => { if (n > 0) e(n - 1); };");
    let module = accepts("const f = (n: i32) => n + 1;");
    assert_eq!(
        global_type(&module, "f"),
        Type::func(vec![Type::I32], Type::I32)
    );
}
