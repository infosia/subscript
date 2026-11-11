//! A `void` function returns `undefined` (compiler.md §250).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
type CB = () => void;
class Box { v: i32 = 1; }
function u(): Box | undefined { return undefined; }
function run(cb: CB): void { cb(); }
"#;

const PLACES: &str = "`undefined` is legal only as a result `T | undefined`, as a result `void`, and in an absence comparison";

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

/// The body of the function `name`.
fn body_of<'a>(module: &'a hir::Module, name: &str) -> &'a [hir::Stmt] {
    &module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("{name}"))
        .body
}

/// Whether each `return` in `statements` has no value, at each depth of
/// blocks and branches, and one `return` exists.
fn returns_have_no_value(statements: &[hir::Stmt]) -> bool {
    fn walk(statements: &[hir::Stmt], seen: &mut usize) -> bool {
        statements.iter().all(|statement| match statement {
            hir::Stmt::Return { value, .. } => {
                *seen += 1;
                value.is_none()
            }
            hir::Stmt::If { then, els, .. } => {
                walk(then, seen) && els.as_deref().is_none_or(|els| walk(els, seen))
            }
            hir::Stmt::Block(inner) => walk(inner, seen),
            _ => true,
        })
    }
    let mut seen = 0;
    walk(statements, &mut seen) && seen > 0
}

/// Rule 1: in a function, a method, and an async function with the
/// result `void` or `Promise<void>`, `return undefined;` is `return;`,
/// also early and through parentheses.
#[test]
fn return_undefined_in_a_void_function_is_return() {
    for (body, name) in [
        (
            "function f(): void { print(\"f\"); return undefined; }",
            "f",
        ),
        (
            "function f(b: boolean): void { if (b) return undefined; print(\"f\"); }",
            "f",
        ),
        ("function f(): void { return (undefined); }", "f"),
        (
            "async function f(): Promise<void> { print(\"f\"); return undefined; }",
            "f",
        ),
    ] {
        let module = accepts(body);
        assert!(returns_have_no_value(body_of(&module, name)), "{body}");
    }
    accepts("class L { log(b: boolean): void { if (b) { return undefined; } print(\"l\"); } }");
}

/// The body of the first lambda of `f`: the initializer of a `let`, or
/// an argument of a call statement.
fn first_lambda_body(module: &hir::Module) -> Vec<hir::Stmt> {
    let lambda = body_of(module, "f").iter().find_map(|statement| {
        let expr = match statement {
            hir::Stmt::Let { init, .. } => init,
            hir::Stmt::Expr(hir::Expr {
                kind: hir::ExprKind::Call { args, .. },
                ..
            }) => args.first()?,
            _ => return None,
        };
        match &expr.kind {
            hir::ExprKind::Lambda { body, .. } => Some(body.clone()),
            _ => None,
        }
    });
    lambda.expect("a lambda")
}

/// Rules 2 and 3: a lambda with the result `void` from an annotation or
/// from a context takes `return undefined;` and the body `undefined`, and
/// each return has no value: the body `undefined` is empty.
#[test]
fn a_void_lambda_returns_undefined() {
    for body in [
        "function f(): void { const h = (): void => { print(\"h\"); return undefined; }; h(); }",
        "function f(): void { const k = (): void => undefined; k(); }",
        "function f(): void { run(() => undefined); }",
        "function f(): void { run(() => { return undefined; }); }",
        "async function f(): Promise<void> { const a = async (): Promise<void> => { return undefined; }; await a(); }",
    ] {
        let module = accepts(body);
        let lambda = first_lambda_body(&module);
        assert!(returns_have_no_value(&lambda), "{body}: {lambda:?}");
    }
    let module = accepts("function f(): void { const k = (): void => undefined; }");
    assert!(
        first_lambda_body(&module)
            .iter()
            .all(|statement| matches!(statement, hir::Stmt::Return { value: None, .. })),
        "the body `undefined` holds no statement but `return;`"
    );
    accepts("function f(): void { [1].forEach((x: i32) => undefined); }");
}

/// Rule 3: each leaf of a conditional that §237 splits is a return, and
/// a leaf `undefined` is `return;`.
#[test]
fn a_conditional_leaf_returns_undefined() {
    for body in [
        "function side(): void {}\nfunction f(b: boolean): void { return b ? undefined : side(); }",
        "function f(b: boolean): void { return b ? undefined : undefined; }",
    ] {
        let module = accepts(body);
        assert!(returns_have_no_value(body_of(&module, "f")), "{body}");
    }
}

/// Rules 1, 3, and 4: an expression of the type `T | undefined` keeps its
/// diagnostic in a `void` function, a setter and a constructor reject each
/// operand (§237 rule 1), a lambda with no result type keeps S012, and a
/// function with another result keeps S012. The S012 text
/// names the places where `undefined` is legal. The firing controls: the
/// accepted forms above.
#[test]
fn the_other_forms_keep_their_diagnostic() {
    for (body, code, message) in [
        (
            "function f(): void { return u(); }",
            RuleCode::S100,
            "a `void` function cannot return a value".to_string(),
        ),
        (
            "function f(): void { const k = () => undefined; }",
            RuleCode::S012,
            PLACES.to_string(),
        ),
        (
            "function f(): i32 { return undefined; }",
            RuleCode::S012,
            format!("{PLACES}; this function returns `i32`, not `T | undefined`"),
        ),
        (
            "function f(): void { const x = undefined; }",
            RuleCode::S012,
            PLACES.to_string(),
        ),
        (
            "class C { v: i32 = 0; set w(x: i32) { if (x < 0) return undefined; this.v = x; } get w(): i32 { return this.v; } }",
            RuleCode::S012,
            PLACES.to_string(),
        ),
        (
            "class C { constructor() { return undefined; } }",
            RuleCode::S012,
            PLACES.to_string(),
        ),
        (
            "function* g(): Generator<i32> { return undefined; }",
            RuleCode::S100,
            "generator return values are not in the decided surface".to_string(),
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
}
