//! A lambda with no value result is `void` (compiler.md §238).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
const ys: i32[] = [];
const flag: boolean = true;
function v(): void {}
async function w1(): Promise<void> {}
async function wi(): Promise<i32> { return 1; }
function each<T>(xs: T[], f: (x: T) => void): void { for (const x of xs) { f(x); } }
class Walker { visit<T>(items: T[], f: (x: T) => void): void { for (const x of items) { f(x); } } }
function mapOne<T>(x: T, f: (x: T) => T): T { return f(x); }
"#;

const BLOCK_ANNOTATION: &str = "a lambda with a block body requires a return type annotation";

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
}

fn accepts(body: &str) -> hir::Module {
    check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"))
}

/// The one diagnostic of a rejected body.
fn rejects(body: &str) -> Diagnostic {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    errors.into_iter().next().expect("one diagnostic")
}

/// The result type of the first lambda in the module.
fn lambda_result(module: &hir::Module) -> subscript_compiler::Type {
    module
        .globals
        .iter()
        .find_map(|global| match &global.init.kind {
            hir::ExprKind::Lambda { ret, .. } => Some(ret.clone()),
            _ => None,
        })
        .expect("a lambda global")
}

/// Rule 1: an expression body of type `void`, and a block with no
/// `return` operand, give the result `void`, sync or async. The control in
/// the same shape: a block with `return 1` keeps its diagnostic (rule 3).
#[test]
fn a_lambda_with_no_value_result_is_void() {
    for body in [
        "const f = () => v();",
        "const f = async () => v();",
        "const f = () => { v(); };",
        "const f = async () => { await w1(); };",
        "const f = (b: boolean) => { if (b) { return; } v(); };",
    ] {
        let module = accepts(body);
        assert_eq!(
            lambda_result(&module),
            subscript_compiler::Type::Void,
            "{body}"
        );
    }
    // The binding has the type of a function with the result `void`.
    accepts("const f = () => { v(); };\nconst g: () => void = f;");
    let control = rejects("const f = () => { return 1; };");
    assert_eq!(control.code, RuleCode::S100);
    assert_eq!(control.message, BLOCK_ANNOTATION);
}

/// Rules 1 and 3: a `return` in a nested lambda is not a `return` of the
/// block, so the outer lambda is `void`. A bare `return` and a `return 1`
/// in one block keep the diagnostic.
#[test]
fn a_nested_return_does_not_count() {
    let module = accepts("const f = () => { const g = (): i32 => { return 1; }; g(); };");
    assert_eq!(lambda_result(&module), subscript_compiler::Type::Void);
    let diagnostic = rejects("const f = (b: boolean) => { if (b) { return; } return 1; };");
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(diagnostic.message, BLOCK_ANNOTATION);
    // Rule 3: an annotation keeps its type.
    let module = accepts("const f = (): void => { v(); };");
    assert_eq!(lambda_result(&module), subscript_compiler::Type::Void);
}

/// Rule 1 in reaction callbacks: a `void` callback of `then`.
#[test]
fn a_void_reaction_callback_is_accepted() {
    accepts("async function f(): Promise<void> { return w1().then(() => v()); }");
    accepts("async function f(): Promise<void> { await w1().then(() => { v(); }); }");
    accepts("async function f(): Promise<void> { await wi().then((x: i32) => v()); }");
}

/// The first lambda of `expr`, depth first.
fn find_lambda(expr: &hir::Expr) -> Option<&hir::Expr> {
    if matches!(expr.kind, hir::ExprKind::Lambda { .. }) {
        return Some(expr);
    }
    expr.children().into_iter().find_map(|child| match child {
        hir::HirChild::Expr(child) => find_lambda(child),
        hir::HirChild::Stmt(_) => None,
    })
}

/// Rule 2: a generic call gives the result `void` of the parameter type as
/// the context of an arrow with no result annotation, so §237 rule 4
/// discards a value body: the arrow has the type `(i32) => void`, and its
/// body is a statement and a bare `return`. The controls in the same
/// shape: a parameter type with the result `T` gives no `void` context,
/// so a value block keeps its diagnostic, and an async arrow is not a
/// `void` callback.
#[test]
fn a_generic_void_callback_takes_the_void_context() {
    for body in [
        "function f(xs: i32[]): void { each(xs, (x: i32) => v()); }",
        "function f(xs: i32[]): void { each(xs, (x: i32) => ys.push(x)); }",
        "function f(xs: i32[]): void { each(xs, (x: i32) => { v(); }); }",
        "function f(w: Walker): void { w.visit([1], (x: i32) => ys.push(x)); }",
    ] {
        let module = accepts(body);
        let f = module
            .functions
            .iter()
            .find(|function| function.name == "f")
            .expect("f");
        let [hir::Stmt::Expr(call)] = f.body.as_slice() else {
            panic!("{body}: {:?}", f.body);
        };
        let lambda = find_lambda(call).expect("the arrow argument");
        assert_eq!(
            lambda.ty,
            subscript_compiler::Type::func(
                vec![subscript_compiler::Type::I32],
                subscript_compiler::Type::Void
            ),
            "{body}"
        );
        let hir::ExprKind::Lambda { body: lowered, .. } = &lambda.kind else {
            panic!("{body}");
        };
        assert!(
            matches!(
                lowered.as_slice(),
                [
                    ..,
                    hir::Stmt::Expr(_),
                    hir::Stmt::Return { value: None, .. }
                ] | [.., hir::Stmt::Expr(_)]
            ) && !lowered
                .iter()
                .any(|statement| matches!(statement, hir::Stmt::Return { value: Some(_), .. })),
            "{body}: {lowered:?}"
        );
    }
    // A value body is a statement, then a bare `return`.
    let module = accepts("function f(xs: i32[]): void { each(xs, (x: i32) => ys.push(x)); }");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Expr(call)] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    let lambda = find_lambda(call).expect("the arrow argument");
    let hir::ExprKind::Lambda { body: lowered, .. } = &lambda.kind else {
        panic!("{lambda:?}");
    };
    assert!(
        matches!(
            lowered.as_slice(),
            [
                hir::Stmt::Expr(hir::Expr {
                    ty: subscript_compiler::Type::I32,
                    ..
                }),
                hir::Stmt::Return { value: None, .. }
            ]
        ),
        "{lowered:?}"
    );
    let control = rejects("function f(): void { const r = mapOne(1, (x: i32) => { return x; }); }");
    assert_eq!(control.code, RuleCode::S100);
    assert_eq!(control.message, BLOCK_ANNOTATION);
    let control = rejects("function f(xs: i32[]): void { each(xs, async (x: i32) => 1); }");
    assert_eq!(control.code, RuleCode::S100);
    assert_eq!(
        control.message,
        "type mismatch: the argument expects `(i32) => void`, got `(i32) => Promise<i32>`"
    );
    let control = rejects("function f(): void { const g = (x: i32) => { return x; }; }");
    assert_eq!(control.message, BLOCK_ANNOTATION);
}
