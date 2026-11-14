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
    // compiler.md §247 rule 1: the `void` place drops the handle result.
    let control = rejects("function f(xs: i32[]): void { each(xs, async (x: i32) => 1); }");
    assert_eq!(control.code, RuleCode::S013);
    assert_eq!(
        control.message,
        "function value returns `Promise<i32>`, which the place `(i32) => void` drops, and no holder awaits the handle; a place with the result `Promise<i32>` can await it"
    );
    let control = rejects("function f(): void { const g = (x: i32) => { return x; }; }");
    assert_eq!(control.message, BLOCK_ANNOTATION);
}

/// compiler.md §286 rules 1 to 4: void operands decide a block result;
/// value operands give one annotation diagnostic per lambda.
#[test]
fn return_operands_must_each_be_void() {
    for body in [
        "const f = (c: boolean) => { if (c) return v(); return 1; };",
        "const f = () => { return 1; };",
        "const f = () => { return \"s\"; };",
        "const f = (c: boolean) => { if (c) return 1; return \"s\"; };",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100);
        assert_eq!(diagnostic.message, BLOCK_ANNOTATION);
    }
    for body in [
        "const f = (c: boolean) => { if (c) return v(); return v(); };",
        "const f = (c: boolean) => { if (c) return v(); };",
    ] {
        let module = accepts(body);
        let f = module
            .globals
            .iter()
            .find(|global| global.name == "f")
            .unwrap();
        assert_eq!(
            f.init.ty,
            subscript_compiler::Type::func(
                vec![subscript_compiler::Type::Bool],
                subscript_compiler::Type::Void
            )
        );
    }
    accepts("const f = async () => { return await w1(); };");
    accepts("const f: () => void = () => { return 1; };");
}

/// compiler.md §286 rule 3a: a rejected result never reaches callers.
#[test]
fn syntactic_value_returns_give_one_annotation_diagnostic() {
    for body in [
        "const f = () => { return 1; }; const n: i32 = f();",
        "const fact = (n: i32) => { if (n <= 1) return 1; return n * fact(n - 1); };",
        "function outer(): void { const fact = (n: i32) => { if (n <= 1) return 1; return n * fact(n - 1); }; }",
        "const f = () => { return undefined; };",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100);
        assert_eq!(diagnostic.message, BLOCK_ANNOTATION);
    }
}

/// compiler.md §286 rule 3b: both calls use a void conditional return.
#[test]
fn conditional_void_calls_are_accepted() {
    let module =
        accepts("const c1 = (c: boolean) => { return c ? v() : v(); }; c1(true); c1(false);");
    assert_eq!(lambda_result(&module), subscript_compiler::Type::Void);
}

/// compiler.md §287 rules 1 to 3: only async void handles are adopted.
#[test]
fn an_async_void_lambda_returns_a_promise_of_void() {
    for body in [
        "const f = () => { return w1(); };",
        "const f = async () => { return wi(); };",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100);
        assert_eq!(diagnostic.message, BLOCK_ANNOTATION);
    }
    let module = accepts("const f = async () => { return w1(); };");
    let f = module
        .globals
        .iter()
        .find(|global| global.name == "f")
        .unwrap();
    assert_eq!(
        f.init.ty,
        subscript_compiler::Type::func(
            vec![],
            subscript_compiler::Type::async_handle(subscript_compiler::Type::Void)
        )
    );
}

/// compiler.md §286 rule 3: an optional-chain return gives one diagnostic.
#[test]
fn an_optional_chain_return_preserves_its_statement() {
    let source = format!(
        "{DECLARATIONS}\nclass O {{ m(): Promise<void> {{ return w1(); }} }}
        export async function main(): Promise<void> {{
            const o: O | null = new O();
            const oc = async () => {{ return o?.m(); }};
            await oc();
        }}"
    );
    let errors = check_program(&[SourceFile::new("test.ts", source.as_str())])
        .expect_err("optional-chain return requires an annotation");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, BLOCK_ANNOTATION);
}
