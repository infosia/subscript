//! A value lambda and a conditional take a `void` result place
//! (compiler.md §242).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
type CB = (x: i32) => void;
class B { v: i32 = 1; }
function named(x: i32): i32 { return x * 2; }
function other(x: i32): void {}
function two(x: i32, i: i32): boolean { return i > 0; }
class K { static twice(x: i32): i32 { return x * 2; } }
class H { f: CB = (x: i32): i32 => x; }
function each(xs: i32[], f: (x: i32) => void): void { for (const x of xs) { f(x); } }
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

/// Rule 1: the callback place of a built-in method reaches each branch of
/// a conditional, with the index when a branch takes it. The control: a
/// conditional whose branches take different counts of parameters stays
/// rejected.
#[test]
fn a_built_in_callback_place_reaches_each_branch() {
    for body in [
        "function f(b: boolean): void { [1].forEach(b ? named : K.twice); }",
        "function f(b: boolean): void { [2].forEach(b ? other : named); }",
        "function f(b: boolean): void { [3].forEach(!b ? (x: i32): i32 => x : named); }",
        "function f(b: boolean, m: Map<string, i32>): void { m.forEach(b ? (v: i32, k: string): void => {} : (v: i32, k: string): i32 => v); }",
        "function f(b: boolean): void { [4].forEach(b ? two : (x: i32, i: i32): i32 => x); }",
        "function f(b: boolean): void { [5].forEach(b ? (x: i32, i: i32): void => {} : (x: i32, i: i32): void => {}); }",
    ] {
        accepts(body);
    }
    // A function with fewer parameters does not fit a function type with
    // more parameters, so branches of two counts of parameters have no
    // common type and stay rejected. `tsc` accepts this form.
    let control =
        rejects("function f(b: boolean): void { [6].forEach(b ? two : (x: i32): void => {}); }");
    assert_eq!(control.code, RuleCode::S100, "{control:?}");
}

/// Rule 1: the type of each branch decides the arity of the place, for each
/// kind of branch: a static method, a field, an element, a module `const`,
/// and a call result, with the index and without it, for `forEach` and
/// `filter`.
#[test]
fn the_type_of_each_branch_decides_the_arity() {
    let declarations = "class S {
  static two(x: i32, i: i32): void {}
  static one(x: i32): void {}
  static p(x: i32, i: i32): boolean { return i > 0; }
  static q(x: i32): boolean { return x > 1; }
}
class F { f: (x: i32, i: i32) => void = S.two; g: (x: i32) => void = S.one; }
const g2 = S.two;
const g1 = S.one;
function mk2(): (x: i32, i: i32) => void { return S.two; }
function mk1(): (x: i32) => void { return S.one; }";
    for branch in [
        "S.two", "h.f", "cbs[0]", "g2", "mk2()", "S.one", "h.g", "cb1[0]", "g1", "mk1()",
    ] {
        accepts(&format!(
            "{declarations}\nfunction f(b: boolean, h: F, cbs: ((x: i32, i: i32) => void)[], cb1: ((x: i32) => void)[]): void {{ [1].forEach(b ? {branch} : {branch}); }}"
        ));
    }
    for branch in ["S.p", "S.q"] {
        accepts(&format!(
            "{declarations}\nfunction f(b: boolean): i32 {{ return [1, 2].filter(b ? {branch} : {branch}).length; }}"
        ));
    }
}

/// Rules 1 and 2: the `finally` callback is a place of a built-in method.
/// The control: an async callback keeps its S013.
#[test]
fn a_finally_callback_takes_the_place() {
    let declarations = "async function work(): Promise<void> {}
function tick(): boolean { return true; }
async function atick(): Promise<void> {}";
    for body in [
        "async function f(): Promise<void> { await work().finally((): i32 => { return 1; }); }",
        "async function f(b: boolean): Promise<void> { await work().finally(b ? tick : (): void => {}); }",
    ] {
        accepts(&format!("{declarations}\n{body}"));
    }
    for body in [
        "async function f(): Promise<void> { await work().finally(async (): Promise<void> => {}); }",
        "async function f(): Promise<void> { await work().finally(atick); }",
    ] {
        let diagnostic = rejects(&format!("{declarations}\n{body}"));
        assert_eq!(diagnostic.code, RuleCode::S013, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message,
            "a `finally` callback must be `() => void`; it is `() => Promise<void>`",
            "{body}"
        );
    }
}

/// Rules 2 and 4: a lambda with a value result annotation takes each place
/// whose function type has the result `void`.
#[test]
fn a_value_lambda_takes_each_void_result_place() {
    for body in [
        "function f(): void { each([6], (x: i32): i32 => x); }",
        "function f(): void { [7].forEach((x: i32): i32 => { return x; }); }",
        "function f(): void { const cb: CB = (x: i32): i32 => x; }",
        "function f(b: boolean): void { const r: CB = b ? (x: i32): i32 => x : other; }",
        "function f(): CB { return (x: i32): i32 => x; }",
        "function f(h: H): void { h.f = (x: i32): i32 => x; }",
        "function f(): void { const xs: CB[] = [(x: i32): i32 => x]; }",
        "function f(base: CB[]): void { const xs: CB[] = [...base, (x: i32): i32 => x]; }",
    ] {
        accepts(body);
    }
}

/// Rule 3: the body is checked against the annotation, then the lambda has
/// the result `void`, and `return e;` is `e; return;`.
#[test]
fn the_lambda_has_the_result_void() {
    let module = accepts(
        "function f(): void { const cb: CB = (x: i32): i32 => { if (x > 0) { return named(x); } return 0; }; }",
    );
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Let { init, .. }] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    let lambda = find_lambda(init).expect("the lambda");
    assert_eq!(lambda.ty, Type::func(vec![Type::I32], Type::Void));
    let hir::ExprKind::Lambda { ret, body, .. } = &lambda.kind else {
        panic!("{lambda:?}");
    };
    assert_eq!(*ret, Type::Void);
    let [hir::Stmt::If { then, .. }, hir::Stmt::Expr(zero), hir::Stmt::Return { value: None, .. }] =
        body.as_slice()
    else {
        panic!("{body:?}");
    };
    assert_eq!(zero.ty, Type::I32);
    assert!(
        matches!(
            then.as_slice(),
            [
                hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Call { .. },
                    ..
                }),
                hir::Stmt::Return { value: None, .. }
            ]
        ),
        "{then:?}"
    );
    // A return operand that does not fit the annotation keeps its
    // diagnostic.
    let diagnostic = rejects("function f(): void { const cb: CB = (x: i32): i32 => \"s\"; }");
    assert_eq!(diagnostic.code, RuleCode::S100, "{diagnostic:?}");
    assert_eq!(
        diagnostic.message,
        "type mismatch: the lambda body expects `i32`, got `string`"
    );
}

/// Rule 5: an async lambda keeps its diagnostic, and a `T | undefined`
/// annotation keeps its diagnostic. A generator lambda has no source form:
/// an arrow is never a generator.
#[test]
fn an_async_lambda_and_an_undefined_result_keep_their_diagnostic() {
    let diagnostic =
        rejects("function f(): void { each([1], async (x: i32): Promise<i32> => x); }");
    assert_eq!(diagnostic.code, RuleCode::S100, "{diagnostic:?}");
    assert_eq!(
        diagnostic.message,
        "type mismatch: the argument expects `(i32) => void`, got `(i32) => Promise<i32>`"
    );
    // A result that carries a handle keeps its diagnostic: a discarded
    // handle has no holder that awaits it.
    let diagnostic = rejects(
        "function f(m: Map<i32, i32>): void { m.forEach((v: i32, k: i32): Promise<i32>[] => []); }",
    );
    assert_eq!(
        diagnostic.message,
        "`Map.forEach` cannot carry a counted callback result (§171)"
    );
    let errors =
        check("function f(): void { const cb: CB = (x: i32): B | undefined => undefined; }")
            .expect_err("a `T | undefined` annotation");
    assert!(
        errors.iter().any(|error| error.message
            == "`T | undefined` is legal only as the result type of a function declaration or a class method"),
        "{errors:?}"
    );
}

/// Rule 1a: each leaf of a conditional, at any depth, takes the place of
/// its own arity. A nested conditional, a call of a generic function, and a
/// lambda leaf each keep the arity of their own type. The control: leaves
/// of two arities stay rejected.
#[test]
fn each_leaf_takes_the_place_of_its_own_arity() {
    let declarations = "function tv(x: i32, i: i32): void {}
function two2(x: i32, i: i32): void {}
function p2(x: i32, i: i32): boolean { return i > 0; }
function id<F>(f: F): F { return f; }";
    for body in [
        "function f(b: boolean, c: boolean): void { [1, 2].forEach(b ? (c ? tv : two2) : tv); }",
        "function f(b: boolean, c: boolean): i32 { return [1, 2].filter(b ? (c ? p2 : p2) : p2).length; }",
        "function f(b: boolean, c: boolean): void { [1, 2].forEach(b ? tv : (c ? two2 : tv)); }",
        "function f(b: boolean, c: boolean): void { [1, 2].forEach(b ? id(c ? tv : two2) : tv); }",
        "function f(b: boolean, c: boolean): void { [1, 2].forEach(b ? (c ? (x: i32, i: i32): void => {} : tv) : two2); }",
        // Value lambdas and named functions with a value result, nested.
        "function f(b: boolean, c: boolean): void { [3].forEach(b ? (c ? (x: i32): i32 => x : named) : K.twice); }",
        "function f(b: boolean, c: boolean): void { [3].forEach(b ? (c ? (x: i32, i: i32): i32 => i : two) : (x: i32, i: i32): boolean => true); }",
    ] {
        accepts(&format!("{declarations}\n{body}"));
    }
    let control = rejects(&format!(
        "{declarations}\nfunction f(b: boolean, c: boolean): void {{ [4].forEach(b ? (c ? tv : named) : two2); }}"
    ));
    assert_eq!(control.code, RuleCode::S100, "{control:?}");
}

/// Rule 5: a result that holds a handle at any depth keeps the diagnostic
/// of its place, for a lambda and for a §240 reference. The predicate is
/// the one of the drop check (§188.1 rule 1). The control: the same places
/// accept a result that holds no handle.
#[test]
fn a_result_that_holds_a_handle_keeps_its_diagnostic() {
    let declarations = "async function one(x: i32): Promise<i32> { return x; }
function mk(x: i32): Promise<i32>[][] { return [[one(x)]]; }
function plain(x: i32): i32[][] { return [[x]]; }";
    for (body, code, message) in [
        (
            "function f(): void { const cb: CB = (x: i32): Promise<i32>[][] => [[one(x)]]; }",
            RuleCode::S100,
            "type mismatch: the initializer expects `(i32) => void`, got `(i32) => Promise<i32>[][]`",
        ),
        (
            "function f(): void { const cb: CB = (x: i32): FixedArray<Promise<i32>, 1> => [one(x)]; }",
            RuleCode::S100,
            "type mismatch: the initializer expects `(i32) => void`, got `(i32) => FixedArray<Promise<i32>, 1>`",
        ),
        (
            "function f(): void { const cb: CB = mk; }",
            RuleCode::S100,
            "type mismatch: the initializer expects `(i32) => void`, got `(i32) => Promise<i32>[][]`",
        ),
        (
            "function f(): void { [1].forEach(mk); }",
            RuleCode::S014,
            "`forEach` cannot carry a counted callback result (§171)",
        ),
    ] {
        let diagnostic = rejects(&format!("{declarations}\n{body}"));
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    for body in [
        "function f(): void { const cb: CB = (x: i32): i32[][] => [[x]]; }",
        "function f(): void { const cb: CB = plain; }",
        "function f(): void { [1].forEach(plain); }",
    ] {
        accepts(&format!("{declarations}\n{body}"));
    }
}
