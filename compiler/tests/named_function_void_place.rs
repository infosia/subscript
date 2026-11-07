//! A named function takes a `void` result place (compiler.md §240).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
function named(x: i32): i32 { return x * 2; }
function two(x: i32, i: i32): boolean { return i > 0; }
function nothing(x: i32): void {}
class K { static twice(x: i32): i32 { return x * 2; } }
class Holder { f: (x: i32) => void = named; }
function each(xs: i32[], f: (x: i32) => void): void { for (const x of xs) { f(x); } }
async function an(x: i32): Promise<void> {}
function gen<T>(x: T): T { return x; }
class B { v: i32 = 1; }
function undef(x: i32): B | undefined { return undefined; }
function nb(b: B | null): void {}
type Cb = (x: i32) => void;
async function one(): Promise<i32> { return 1; }
function tick(): boolean { return true; }
async function atick(): Promise<void> {}
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

/// Rule 1: a function declaration or a static method with a value result
/// takes each §235 rule 5a place whose function type has the result
/// `void`.
#[test]
fn a_named_function_takes_a_void_result_place() {
    for body in [
        "function f(xs: i32[]): void { xs.forEach(named); }",
        "function f(xs: i32[]): void { xs.forEach(two); }",
        "function f(xs: i32[]): void { each(xs, named); }",
        "function f(xs: i32[]): void { each(xs, K.twice); }",
        "function f(): void { const cb: (x: i32) => void = named; }",
        "function f(h: Holder): void { h.f = K.twice; }",
        "function f(): (x: i32) => void { return named; }",
    ] {
        accepts(body);
    }
}

/// Rule 2: the place holds a lambda with no environment and the
/// parameters of the function; its body calls the function with its
/// arguments and discards the result.
#[test]
fn the_place_holds_a_discarding_lambda() {
    let module = accepts("function f(): void { const cb: (x: i32) => void = named; }");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Let { init, .. }] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    assert_eq!(init.ty, Type::func(vec![Type::I32], Type::Void));
    let hir::ExprKind::Lambda {
        params,
        is_async,
        owns_environment,
        ret,
        body,
        captures,
        ..
    } = &init.kind
    else {
        panic!("{init:?}");
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].ty, Type::I32);
    assert!(!is_async && !owns_environment && captures.is_empty());
    assert_eq!(*ret, Type::Void);
    let [hir::Stmt::Expr(hir::Expr {
        kind:
            hir::ExprKind::Call {
                callee: hir::Callee::Func(symbol),
                args,
            },
        ty: Type::I32,
        ..
    })] = body.as_slice()
    else {
        panic!("{body:?}");
    };
    assert!(symbol.full_text().ends_with("named"), "{symbol:?}");
    assert!(
        matches!(args.as_slice(), [hir::Expr { kind: hir::ExprKind::Local(name, ..), .. }] if *name == params[0].name),
        "{args:?}"
    );
    // The control: a function with the result `void` is the reference
    // itself.
    let module = accepts("function f(): void { const cb: (x: i32) => void = nothing; }");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    assert!(
        matches!(
            f.body.as_slice(),
            [hir::Stmt::Let {
                init: hir::Expr {
                    kind: hir::ExprKind::FuncRef(_),
                    ..
                },
                ..
            }]
        ),
        "{:?}",
        f.body
    );
}

/// Rule 4 with compiler.md §246: a parameter and a `let` local take no
/// adapter and name the cause (§246 rule 5); an async function and a
/// generic function keep their present diagnostic. A const local takes the
/// adapter (§246 rule 1).
#[test]
fn a_value_with_an_environment_an_async_or_a_generic_function_stays_rejected() {
    accepts("function f(): void { const l = (x: i32): i32 => x; const cb: (x: i32) => void = l; }");
    for (body, message) in [
        (
            "function f(fv: (x: i32) => i32): void { const cb: (x: i32) => void = fv; }",
            "function value `fv` needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it",
        ),
        (
            "function f(): void { let l = (x: i32): i32 => x; const cb: (x: i32) => void = l; }",
            "function value `l` needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it",
        ),
        (
            "function f(xs: i32[]): void { each(xs, an); }",
            "type mismatch: the argument expects `(i32) => void`, got `(i32) => Promise<void>`",
        ),
        (
            "function f(xs: i32[]): void { each(xs, gen); }",
            "generic function `gen` has no first-class value; call it directly or use a lambda",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
}

/// Rule 2a: `??`, `??=`, `||=`, and `&&=` decide with the check that
/// takes the value, so a named function takes the place there too. The
/// HIR of `handler ??= named` holds the lambda.
#[test]
fn a_logical_place_decides_with_the_value() {
    for body in [
        "function f(): void { let handler: ((x: i32) => void) | null = null; handler ??= named; }",
        "function f(n: ((x: i32) => void) | null): void { const g = n ?? named; g(1); }",
        "function f(n: ((x: i32) => void) | null): void { const g: (x: i32) => void = n ?? named; g(1); }",
        "function f(): void { let g: ((x: i32) => void) | null = null; g = g ?? named; }",
    ] {
        accepts(body);
    }
    let module =
        accepts("function f(n: ((x: i32) => void) | null): void { const g = n ?? named; g(1); }");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Let { init, ty, .. }, ..] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    assert_eq!(*ty, Type::func(vec![Type::I32], Type::Void));
    let lambda = find_lambda(init).expect("the right operand is the lambda of rule 2");
    assert_eq!(lambda.ty, Type::func(vec![Type::I32], Type::Void));
    // `||=` and `&&=` require a boolean operand (§199.1 rule 6), as they do
    // with a `void` function on the right.
    for (body, spelling) in [
        (
            "function f(): void { let g: (x: i32) => void = nothing; g &&= named; }",
            "&&=",
        ),
        (
            "function f(): void { let g: (x: i32) => void = nothing; g ||= named; }",
            "||=",
        ),
        (
            "function f(): void { let g: (x: i32) => void = nothing; g &&= nothing; }",
            "&&=",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message,
            format!("`{spelling}` requires a boolean operand, got `(i32) => void`"),
            "{body}"
        );
    }
}

/// Rule 1: a function whose result is `T | undefined` keeps its C7
/// diagnostic in a place with the result `void` (§240.4 item 3).
#[test]
fn an_undefined_result_keeps_its_diagnostic() {
    let diagnostic = rejects("function f(): void { const cb: (x: i32) => void = undef; }");
    assert_eq!(diagnostic.code, RuleCode::S100, "{diagnostic:?}");
    assert_eq!(
        diagnostic.message,
        "a function whose result is `T | undefined` may only be called"
    );
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

/// Rule 2: an array-literal element beside a spread and a `finally`
/// callback are places too. The controls: an async function stays
/// rejected in each place.
#[test]
fn a_spread_element_and_a_finally_callback_are_places() {
    for body in [
        "function f(base: Cb[]): void { const all: Cb[] = [...base, named]; }",
        "function f(base: ((b: B) => void)[]): void { const all: ((b: B) => void)[] = [...base, nb]; }",
        "async function f(): Promise<void> { print(`${await one().finally(tick)}`); }",
    ] {
        accepts(body);
    }
    let module = accepts("function f(base: Cb[]): void { const all: Cb[] = [...base, named]; }");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Let { init, .. }] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    let hir::ExprKind::ArraySpreadLit(elements) = &init.kind else {
        panic!("{init:?}");
    };
    let [_, element] = elements.as_slice() else {
        panic!("{elements:?}");
    };
    assert!(element.spread.is_none());
    assert!(
        matches!(element.expr.kind, hir::ExprKind::Lambda { .. }),
        "{:?}",
        element.expr
    );
    assert_eq!(element.expr.ty, Type::func(vec![Type::I32], Type::Void));
    let control = rejects("function f(base: Cb[]): void { const all: Cb[] = [...base, an]; }");
    assert_eq!(control.code, RuleCode::S100, "{control:?}");
    assert_eq!(
        control.message,
        "type mismatch: the array element expects `(i32) => void`, got `(i32) => Promise<void>`"
    );
    let control =
        rejects("async function f(): Promise<void> { print(`${await one().finally(atick)}`); }");
    assert_eq!(control.code, RuleCode::S013, "{control:?}");
    assert_eq!(
        control.message,
        "a `finally` callback must be `() => void`; it is `() => Promise<void>`"
    );
}

/// Rule 1: a function whose result carries a handle is not converted, and
/// keeps the diagnostic of its place: a discarded handle has no holder
/// that awaits it. The control: a function with a value result is accepted
/// in the same places.
#[test]
fn a_result_that_carries_a_handle_keeps_its_diagnostic() {
    let jobs = "function jobs(x: i32): Promise<i32>[] { return []; }";
    for (body, code, message) in [
        (
            "function f(): void { each([1], jobs); }",
            RuleCode::S100,
            "type mismatch: the argument expects `(i32) => void`, got `(i32) => Promise<i32>[]`",
        ),
        (
            "function f(): void { [1].forEach(jobs); }",
            RuleCode::S014,
            "`forEach` cannot carry a counted callback result (§171)",
        ),
    ] {
        let diagnostic = rejects(&format!("{jobs}\n{body}"));
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    for body in [
        "function f(): void { each([1], named); }",
        "function f(): void { [1].forEach(named); }",
    ] {
        accepts(body);
    }
}
