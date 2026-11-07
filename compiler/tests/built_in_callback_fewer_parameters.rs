//! A built-in callback takes a function with fewer parameters
//! (compiler.md §244).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
function none(): void {}
function one(x: i32): void {}
function two(x: i32, i: i32): void {}
function d(x: i32 = 1): void {}
function gen<T>(): void {}
async function an(): Promise<void> {}
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

/// Rule 1: each callback place with a stated arity takes a lambda literal
/// or a reference with fewer parameters, at its smallest arity.
#[test]
fn a_callback_takes_fewer_parameters() {
    for body in [
        "function f(): void { [1, 2].forEach(() => {}); }",
        "function f(): void { [1].forEach(none); }",
        "function f(): i32 { return [1, 2].map((): i32 => 0).length; }",
        "function f(): i32 { return [1, 2, 3].filter((): boolean => true).length; }",
        "function f(m: Map<string, i32>): void { m.forEach((v: i32) => {}); }",
        "function f(m: Map<string, i32>): void { m.forEach(() => {}); }",
        "function f(m: Map<string, i32>): void { m.forEach(one); }",
        "function f(s: Set<i32>): void { s.forEach(() => {}); }",
        "function f(): i32 { return [1, 2].reduce((acc: i32): i32 => acc + 1, 0); }",
        "function f(): i32 { return [2, 1].sort((): i32 => 0).length; }",
        "function f(): i32 { return Map.groupBy([1, 2], (): string => \"k\").size; }",
        "function f(): void { const fa: FixedArray<i32, 2> = [1, 2]; fa.forEach(() => {}); }",
    ] {
        accepts(body);
    }
    // The padded lambda has the parameters of the place.
    let module = accepts("function f(): void { [1].forEach(none); }");
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Expr(call)] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    let lambda = find_lambda(call).expect("the lambda of the reference");
    assert_eq!(lambda.ty, Type::func(vec![Type::I32], Type::Void));
}

/// Rule 2: the leaves of a conditional take the place of their largest
/// arity, at any depth. The control: a leaf with more parameters than the
/// place allows keeps the diagnostic.
#[test]
fn the_leaves_take_the_largest_arity() {
    for body in [
        "function f(b: boolean): void { [6].forEach(b ? two : (x: i32): void => {}); }",
        "function f(b: boolean): void { [7].forEach(b ? one : none); }",
        "function f(b: boolean, c: boolean): void { [8].forEach(b ? (c ? one : none) : two); }",
        "function f(b: boolean): void { [9].forEach(b ? none : (x: i32, i: i32): void => {}); }",
    ] {
        accepts(body);
    }
    let control = rejects(
        "function f(b: boolean): void { [9].forEach(b ? none : (x: i32, i: i32, j: i32): void => {}); }",
    );
    assert_eq!(control.code, RuleCode::S100, "{control:?}");
    // The same leaf in a nested conditional gives one diagnostic.
    let control = rejects(
        "function f(b: boolean, c: boolean): void { [9].forEach(b ? (c ? two : (x: i32, i: i32, j: i32): void => {}) : two); }",
    );
    assert_eq!(
        control.message,
        "conditional branches have no common type: `(i32, i32) => void` and `(i32, i32, i32) => void`"
    );
}

/// Rule 4: more parameters than the place allows, a function value with an
/// environment, a generic function, an async function, and a function with
/// a default parameter keep their diagnostic. The controls: the same
/// places with an accepted callback.
#[test]
fn the_other_forms_keep_their_diagnostic() {
    for (body, code, message) in [
        (
            "function f(m: Map<string, i32>): void { m.forEach((v: i32, k: string, all: Map<string, i32>): void => {}); }",
            RuleCode::S014,
            "`Map.forEach` callbacks take exactly 2 parameter(s); extra lib callback parameters are not accepted (Q24)",
        ),
        (
            "function f(): i32 { return [2, 1].sort((a: i32, b: i32, i: i32): i32 => a - b).length; }",
            RuleCode::S014,
            "`sort` callbacks take exactly 2 parameter(s); got 3 (Q22)",
        ),
        (
            "function f(fv: () => void): void { [1].forEach(fv); }",
            RuleCode::S014,
            "`forEach` callback `fv` is a function value with fewer parameters than the place; an adapter for it needs a new environment, which no rule gives an owner; use a lambda that calls it",
        ),
        (
            "function f(): void { [1].forEach(gen); }",
            RuleCode::S100,
            "generic function `gen` has no first-class value; call it directly or use a lambda",
        ),
        (
            "function f(): void { [1].forEach(an); }",
            RuleCode::S014,
            "`forEach` cannot carry a counted callback result (§171)",
        ),
        (
            "function f(m: Map<string, i32>): void { m.forEach(d); }",
            RuleCode::S014,
            "`Map.forEach` callback `d` has fewer parameters than the place and a default parameter, which has no adapter; use a lambda that calls it",
        ),
        (
            "function f(): void { [1].forEach(async (): Promise<void> => {}); }",
            RuleCode::S014,
            "`forEach` cannot carry a counted callback result (§171)",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    for body in [
        "function f(m: Map<string, i32>): void { m.forEach((v: i32, k: string): void => {}); }",
        "function f(): i32 { return [2, 1].sort((a: i32, b: i32): i32 => a - b).length; }",
        "function f(fv: (x: i32) => void): void { [1].forEach(fv); }",
        "function f(): void { [1].forEach(d); }",
    ] {
        accepts(body);
    }
}

/// Rule 4a: a value with fewer parameters that rule 1 does not take names
/// its cause. The controls in the same shape: the same value with the
/// arity of the place, and a lambda that calls it.
#[test]
fn an_unadapted_value_names_its_cause() {
    for (body, message) in [
        (
            "function f(): void { const xs: i32[] = [1]; const g = (): void => print(\"e\"); xs.forEach(g); }",
            "`forEach` callback `g` is a function value with fewer parameters than the place; an adapter for it needs a new environment, which no rule gives an owner; use a lambda that calls it",
        ),
        (
            "function dr(acc: i32 = 3): i32 { return acc; }\nfunction f(): i32 { const xs: i32[] = [1]; return xs.reduce(dr, 0); }",
            "`reduce` callback `dr` has fewer parameters than the place and a default parameter, which has no adapter; use a lambda that calls it",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S014, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    for body in [
        "function f(): void { const xs: i32[] = [1]; const g = (x: i32): void => print(\"e\"); xs.forEach(g); }",
        "function f(): void { const xs: i32[] = [1]; const g = (): void => print(\"e\"); xs.forEach((x: i32): void => g()); }",
        "function dr(acc: i32, x: i32 = 3): i32 { return acc; }\nfunction f(): i32 { const xs: i32[] = [1]; return xs.reduce(dr, 0); }",
        "function dr(acc: i32 = 3): i32 { return acc; }\nfunction f(): i32 { const xs: i32[] = [1]; return xs.reduce((acc: i32): i32 => dr(acc), 0); }",
    ] {
        accepts(body);
    }
}

/// Rule 4a: a value with fewer parameters that rule 1 does not take, and
/// that has no cause of its own, gets the type mismatch of the place. No
/// such value reaches the arity diagnostic. A conditional gets the
/// diagnostic of its branches. The controls: the same places with a value
/// that fits.
#[test]
fn a_fewer_parameter_value_that_does_not_fit_gets_the_type_mismatch() {
    let declarations = "function sone(x: string): void {}
function zero(): i32 { return 0; }
function acc1(acc: i32): i32 { return acc; }
function dacc(acc: i32 = 3): i32 { return acc; }";
    for (body, message) in [
        (
            "function f(m: Map<string, i32>): void { m.forEach((k: string) => print(k)); }",
            "type mismatch: the `Map.forEach` callback expects `(i32, string)` => void, got `(string) => void`",
        ),
        (
            "function f(m: Map<string, i32>): void { m.forEach(sone); }",
            "type mismatch: the `Map.forEach` callback expects `(i32, string)` => void, got `(string) => void`",
        ),
        (
            "function f(xs: i32[]): void { xs.sort((a: string): i32 => 0); }",
            "type mismatch: the `sort` callback expects `(i32, i32)` => i32, got `(string) => i32`",
        ),
        (
            "function f(): i32 { return [1, 2, 3].filter(zero).length; }",
            "type mismatch: the `filter` callback expects `(i32) or (i32, i32)` => boolean, got `() => i32`",
        ),
        (
            "function f(b: boolean): i32 { return [1, 2].reduce(b ? acc1 : dacc, 0); }",
            "type mismatch: the `reduce` callback expects `(i32, i32) or (i32, i32, i32)` => i32, got `(i32) => i32`",
        ),
    ] {
        let diagnostic = rejects(&format!("{declarations}\n{body}"));
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
        assert!(
            !diagnostic.message.contains("callbacks take"),
            "{body}: the arity diagnostic"
        );
    }
    for body in [
        "function f(m: Map<string, i32>): void { m.forEach((v: i32) => print(`${v}`)); }",
        "function f(xs: i32[]): void { xs.sort((a: i32): i32 => 0); }",
        "function f(): i32 { return [1, 2, 3].filter((): boolean => true).length; }",
        "function add2(acc: i32, x: i32): i32 { return acc + x; }\nfunction f(b: boolean): i32 { return [1, 2].reduce(b ? add2 : add2, 0); }",
    ] {
        accepts(&format!("{declarations}\n{body}"));
    }
}
