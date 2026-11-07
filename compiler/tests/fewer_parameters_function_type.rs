//! A function with fewer parameters takes a function type
//! (compiler.md §243).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
type CB2 = (x: i32, i: i32) => void;
type F2 = (x: i32, i: i32) => i32;
function eachi(xs: i32[], f: (x: i32, i: i32) => void): void {}
function one(x: i32): void {}
function twice(x: i32): i32 { return x * 2; }
class K { static one(x: i32): void {} }
function dflt(x: i32, y: i32 = 1): void {}
function gen<T>(x: T): void {}
async function an(x: i32): Promise<void> {}
function none(): void {}
function inc(acc: i32): i32 { return acc + 1; }
class Foo { v: i32 = 1; }
function eachf(f: (x: Foo, i: i32) => void): void {}
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

/// The initializer of the only `let` in `f`.
fn initializer(module: &hir::Module) -> &hir::Expr {
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Let { init, .. }] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    init
}

/// Rule 1: a lambda literal with fewer parameters takes the trailing
/// parameters of the place, with or without annotations, and with a
/// capture. The lambda then has the parameters of the place.
#[test]
fn a_lambda_takes_the_trailing_parameters() {
    for body in [
        "function f(): void { eachi([1], (x: i32): void => {}); }",
        "function f(): void { eachi([2], (x) => {}); }",
        "function f(): void { eachi([3], () => {}); }",
        "function f(): void { const r: F2 = (x: i32): i32 => x + 1; }",
        "function f(): void { const n: i32 = 10; const g: CB2 = (x: i32): void => print(`${x + n}`); }",
    ] {
        accepts(body);
    }
    let module = accepts("function f(): void { const r: F2 = (x: i32): i32 => x + 1; }");
    let init = initializer(&module);
    assert_eq!(init.ty, Type::func(vec![Type::I32, Type::I32], Type::I32));
    let hir::ExprKind::Lambda { params, .. } = &init.kind else {
        panic!("{init:?}");
    };
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "x");
    assert_eq!(params[1].ty, Type::I32);
}

/// Rule 2: a function declaration or a static method with fewer
/// parameters becomes a lambda with no environment and the parameters of
/// the place, which calls it with its own number of arguments.
#[test]
fn a_named_function_takes_the_parameters_of_the_place() {
    for body in [
        "function f(): void { eachi([4], one); }",
        "function f(): void { eachi([5], K.one); }",
        "function f(): void { eachi([1], twice); }",
        "function f(): void { const t: F2 = twice; }",
        "function f(b: boolean): void { eachi([1], b ? one : twice); }",
    ] {
        accepts(body);
    }
    let module = accepts("function f(): void { const t: F2 = twice; }");
    let init = initializer(&module);
    assert_eq!(init.ty, Type::func(vec![Type::I32, Type::I32], Type::I32));
    let hir::ExprKind::Lambda {
        params,
        captures,
        owns_environment,
        body,
        ..
    } = &init.kind
    else {
        panic!("{init:?}");
    };
    assert_eq!(params.len(), 2);
    assert!(captures.is_empty() && !owns_environment);
    let [hir::Stmt::Return {
        value:
            Some(hir::Expr {
                kind: hir::ExprKind::Call { args, .. },
                ..
            }),
        ..
    }] = body.as_slice()
    else {
        panic!("{body:?}");
    };
    assert!(
        matches!(args.as_slice(), [hir::Expr { kind: hir::ExprKind::Local(name, ..), .. }] if *name == params[0].name),
        "{args:?}"
    );
    // With the result `void`, the value is discarded.
    let module = accepts("function f(): void { const h: CB2 = twice; }");
    let hir::ExprKind::Lambda { body, ret, .. } = &initializer(&module).kind else {
        panic!("a lambda");
    };
    assert_eq!(*ret, Type::Void);
    assert!(
        matches!(
            body.as_slice(),
            [hir::Stmt::Expr(hir::Expr {
                kind: hir::ExprKind::Call { .. },
                ..
            })]
        ),
        "{body:?}"
    );
}

/// Rules 3 and 4: a function value with an environment, a generic
/// function, an async function, a function with a default parameter, and
/// a built-in callback with fewer parameters keep their diagnostic.
#[test]
fn the_other_forms_keep_their_diagnostic() {
    for (body, code, message) in [
        (
            "function f(fv: (x: i32) => void): void { const h: CB2 = fv; }",
            RuleCode::S100,
            "function value `fv` needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it",
        ),
        (
            "function f(): void { eachi([1], gen); }",
            RuleCode::S100,
            "generic function `gen` has no first-class value; call it directly or use a lambda",
        ),
        (
            "function f(): void { eachi([1], an); }",
            RuleCode::S100,
            "type mismatch: the argument expects `(i32, i32) => void`, got `(i32) => Promise<void>`",
        ),
        (
            "function f(): void { const h: (x: i32, y: i32, z: i32) => void = dflt; }",
            RuleCode::S100,
            "type mismatch: the initializer expects `(i32, i32, i32) => void`, got `(i32, i32) => void`",
        ),
        (
            "function f(): void { [1].forEach((x: i32, i: i32, all: i32[]) => print(\"z\")); }",
            RuleCode::S014,
            "`forEach callback with the container parameter` is rejected: Passing the iterated container reference to its callback violates C5's non-escaping-by-construction rule; use `callback(value, index)` (Q27)",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    // The controls in the same places: the same count of parameters.
    for body in [
        "function f(fv: (x: i32, i: i32) => void): void { const h: CB2 = fv; }",
        "function f(): void { const h: (x: i32, y: i32) => void = dflt; }",
        "function f(): void { [1].forEach((x: i32) => print(\"z\")); }",
    ] {
        accepts(body);
    }
}

/// Rule 3 with compiler.md §244: a built-in callback takes a reference with
/// fewer parameters at its smallest arity, and rejects a function value
/// with an environment and a function with a default parameter with the
/// cause (§244 rule 4a). The controls: the references of the place arity.
#[test]
fn a_built_in_callback_pads_a_reference_and_names_an_unadapted_value() {
    for body in [
        "function f(): void { [1, 2].forEach(none); }",
        "function f(b: boolean): void { [1, 2].forEach(b ? one : none); }",
        "function f(): i32 { return [1, 2].reduce(inc, 0); }",
        "function f(): void { new Set<i32>().forEach(none); }",
    ] {
        accepts(body);
    }
    for (body, code, message) in [
        (
            "function f(fv: () => void): void { [1, 2].forEach(fv); }",
            RuleCode::S014,
            "`forEach` callback `fv` needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it",
        ),
        (
            "function d(x: i32 = 1): void {}\nfunction f(m: Map<string, i32>): void { m.forEach(d); }",
            RuleCode::S014,
            "`Map.forEach` callback `d` has fewer parameters than the place and a default parameter, which has no adapter; use a lambda that calls it",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    for body in [
        "function f(): void { [1, 2].forEach(one); }",
        "function f(b: boolean): void { [1, 2].forEach(b ? one : one); }",
        "function f(): void { new Set<i32>().forEach(one); }",
    ] {
        accepts(body);
    }
}

/// Rule 1 with §235 variance: the padded lambda keeps its own parameter
/// types, and the place adds the cast of §235 rule 5a.
#[test]
fn a_padded_lambda_keeps_its_own_parameter_types() {
    let module = accepts(
        "function f(): void { eachf((g: Foo | null): void => { print(`${g !== null}`); }); }",
    );
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [hir::Stmt::Expr(hir::Expr {
        kind: hir::ExprKind::Call { args, .. },
        ..
    })] = f.body.as_slice()
    else {
        panic!("{:?}", f.body);
    };
    let [argument] = args.as_slice() else {
        panic!("{args:?}");
    };
    let hir::ExprKind::Cast(lambda) = &argument.kind else {
        panic!("the place casts the padded lambda: {argument:?}");
    };
    let hir::ExprKind::Lambda { params, .. } = &lambda.kind else {
        panic!("{lambda:?}");
    };
    let own = lambda.ty.function_type().expect("a function type");
    assert_eq!(own.params.len(), 2);
    assert!(matches!(own.params[0], Type::Nullable(..)), "{own:?}");
    assert_eq!(own.params[0], params[0].ty);
    assert_eq!(own.params[1], Type::I32);
    let place = argument.ty.function_type().expect("a function type");
    assert!(!matches!(place.params[0], Type::Nullable(..)), "{place:?}");
}
