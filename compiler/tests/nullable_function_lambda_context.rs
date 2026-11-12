//! A nullable function type gives a lambda its parameter types
//! (compiler.md §251).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
type CB = (x: i32) => void;
function one(x: i32): void { print(`one ${x}`); }
function maybe(): CB | null { return one; }
function three<T>(a: T, b: T, c: T): T { return c; }
function take(f: CB | null): void {}
class Holder { cb: CB | null = null; }
"#;

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

/// The initializer of the `let` named `name` in `f`.
fn initializer<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Expr {
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    f.body
        .iter()
        .find_map(|statement| match statement {
            hir::Stmt::Let {
                name: local, init, ..
            } if local == name => Some(init),
            _ => None,
        })
        .expect("the binding")
}

fn cb() -> Type {
    Type::func(vec![Type::I32], Type::Void)
}

/// Rule 1: `F | null` gives a lambda the parameter types and the result
/// context of `F`, and the lambda has the type `F`.
#[test]
fn a_nullable_function_type_gives_the_parameter_types() {
    let module = accepts("function f(): void { const g: CB | null = (x) => print(`g ${x}`); }");
    let init = initializer(&module, "g");
    let hir::ExprKind::Lambda { params, .. } = &init.kind else {
        panic!("{init:?}");
    };
    assert_eq!(params[0].ty, Type::I32);
    assert_eq!(init.ty, cb());
    let module =
        accepts("function f(): void { const d: ((x: i32) => i32) | null = (x) => x * 2; }");
    let init = initializer(&module, "d");
    assert_eq!(init.ty, Type::func(vec![Type::I32], Type::I32));
    // A lambda that annotates its parameters and not its result takes
    // the result context of `F`.
    let module =
        accepts("function f(): void { const e: ((x: i32) => f64) | null = (x: i32) => 7 / 2; }");
    let init = initializer(&module, "e");
    assert_eq!(init.ty, Type::func(vec![Type::I32], Type::F64));
}

/// Rule 2: each place with a contextual type gives it, at each depth: an
/// assignment, a result, an argument, an argument after the inference of
/// §249 rule 2a in each order, a branch, a member, an element, a
/// parenthesized lambda, and a `Map` value.
#[test]
fn each_place_gives_the_context() {
    for body in [
        "function f(): void { let g: CB | null = null; g = (x) => print(`g ${x}`); }",
        "function f(): CB | null { return (x) => print(`r ${x}`); }",
        "function f(): void { take((x) => print(`t ${x}`)); }",
        "function f(): void { three(one, maybe(), (x) => print(`h ${x}`)); }",
        "function f(): void { three(maybe(), one, (x) => print(`k ${x}`)); }",
        "function f(b: boolean): void { const c: CB | null = b ? (x) => print(`c ${x}`) : null; }",
        "function f(h: Holder): void { h.cb = (x) => print(`h ${x}`); }",
        "function f(): void { const xs: (CB | null)[] = [(x) => print(`xs ${x}`), null]; }",
        "function f(): void { const p: CB | null = ((x) => print(`p ${x}`)); }",
        "function f(): void { const m = new Map<string, CB | null>(); m.set(\"k\", (x) => print(`m ${x}`)); }",
    ] {
        accepts(body);
    }
}

/// Rule 3: a nullable hint gives no parameter types, and a lambda with no
/// context keeps its S100. The firing control: the contextual forms above.
#[test]
fn a_hint_and_no_context_keep_the_diagnostic() {
    for body in [
        "function f(): void { const xs = [maybe(), (x) => print(`h ${x}`)]; }",
        "function f(): void { const k = (x) => print(`k ${x}`); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message, "parameters require a type annotation",
            "{body}"
        );
    }
}

/// Rule 4: an async lambda takes the function type of `F | null`, and its
/// result follows the place with the type `F`.
#[test]
fn an_async_lambda_takes_the_function_type() {
    let module = accepts(
        "async function f(): Promise<void> { const w: ((x: i32) => Promise<void>) | null = async (x) => { print(`w ${x}`); }; }",
    );
    let init = initializer(&module, "w");
    assert_eq!(
        init.ty,
        Type::func(vec![Type::I32], Type::async_handle(Type::Void))
    );
}
