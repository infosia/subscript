//! An element hint gives no parameter types to a lambda (compiler.md
//! §249).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
type CB = (x: i32) => void;
function one(x: i32): void { print(`one ${x}`); }
function pick(): CB | null { return null; }
function take(f: CB): void { f(1); }
function give(): CB { return (x) => print(`give ${x}`); }
function two<T>(a: T, xs: T[]): T[] { return xs; }
function twoR<T>(xs: T[], a: T): T[] { return xs; }
@Descriptor
class D { f!: CB; }
function mkd(): D { return { f: one }; }
function mk(): CB { return one; }
function wrap<T>(f: T): T { return f; }
class Box<T> { f: T; constructor(f: T) { this.f = f; } }
function mkb(): Box<CB> { return new Box<CB>(one); }
function maybe(): CB | null { return null; }
function three<T>(a: T, b: T, c: T): T { return c; }
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

/// Rules 1 and 1a: a lambda with a parameter that has no annotation, with
/// only a hint, gets the diagnostic of a lambda with no context: as an
/// element after a function or an annotated lambda, in a nested list, in a
/// conditional element, in a spread operand, through the result of a
/// lambda element, through a member of a descriptor literal, and in a
/// generic call, a `new`, or `Array.of` that a hint reaches. The
/// conditional with no context gives the same diagnostic.
#[test]
fn a_hinted_lambda_requires_parameter_annotations() {
    for body in [
        "function f(): void { const a1 = [one, (x) => print(`a1 ${x}`)]; }",
        "function f(): void { const a2 = [(x: i32): void => {}, (x) => print(`a2 ${x}`)]; }",
        "function f(): void { const ns = [[one], [(x) => print(`ns ${x}`)]]; }",
        "function f(b: boolean): void { const a3 = [one, b ? one : (x) => print(`a3 ${x}`)]; }",
        "function f(): void { const a4 = [one, ...[(x) => print(`a4 ${x}`)]]; }",
        "function f(b: boolean): void { const c1 = b ? one : (x) => print(`c1 ${x}`); }",
        "function f(): void { const m1 = [(): CB => one, () => (x) => print(`m1 ${x}`)]; }",
        "function f(): void { const m2 = [() => one, () => (x) => print(`m2 ${x}`)]; }",
        "function f(): void { const m3 = [() => one, () => { return (x) => print(`m3 ${x}`); }]; }",
        "function f(): void { const m4 = [(y: i32) => one, (y: i32) => (x) => print(`m4 ${x}`)]; }",
        "function f(): void { const d1 = [mkd(), { f: (x) => print(`d1 ${x}`) }]; }",
        "function f(): void { const d2 = [[mkd()], [{ f: (x) => print(`d2 ${x}`) }]]; }",
        "function f(): void { const w1 = [mk, () => wrap((x) => print(`w1 ${x}`))]; }",
        "function f(): void { const w2 = [mk, () => { return wrap((x) => print(`w2 ${x}`)); }]; }",
        "function f(): void { const b1 = new Box<CB>(one); const n1 = [b1, new Box((x) => print(`n1 ${x}`))]; }",
        "function f(): void { const n2 = [mkb, () => new Box((x) => print(`n2 ${x}`))]; }",
        "function f(): void { const o1 = [[one], Array.of((x) => print(`o1 ${x}`))]; }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message, "parameters require a type annotation",
            "{body}"
        );
    }
}

/// Rule 2: a contextual type from outside the list keeps its rule: an
/// annotation, a nested annotation, a parameter type, a return type, an
/// assignment target, and the right operand of `??`, alone and as an
/// element. The firing control: the same lambdas in a list with no
/// contextual type are rejected above.
#[test]
fn a_contextual_type_from_outside_the_list_types_the_parameters() {
    for body in [
        "function f(): void { const fs: CB[] = [one, (x) => print(`fs ${x}`)]; }",
        "function f(): void { const gs: CB[][] = [[one], [(x) => print(`gs ${x}`)]]; }",
        "function f(): void { take((x) => print(`take ${x}`)); }",
        "function f(): void { const g = give(); }",
        "function f(): void { let hs: CB[] = []; hs = [one, (x) => print(`hs ${x}`)]; }",
        "function f(): void { const n1 = pick() ?? ((x) => print(`n1 ${x}`)); }",
        "function f(): void { const a6 = [one, pick() ?? ((x) => print(`a6 ${x}`))]; }",
        "function f(): void { const ds: D[] = [mkd(), { f: (x) => print(`ds ${x}`) }]; }",
        "function f(): void { const gs: (() => CB)[] = [() => one, () => (x) => print(`gs ${x}`)]; }",
        "function f(): void { const m5 = [() => one, (): CB => { return (x) => print(`m5 ${x}`); }]; }",
        "function f(): void { const w3: CB[] = [one, wrap((x) => print(`w3 ${x}`))]; }",
        "function f(): void { const w4 = [mk, (): CB => { return wrap((x) => print(`w4 ${x}`)); }]; }",
    ] {
        accepts(body);
    }
}

/// Rule 2a: an argument of a generic call takes the type of its
/// parameter, with the type parameter that another argument fixes, in
/// either order, as an explicit type argument gives it. The controls: the
/// explicit type arguments.
#[test]
fn a_generic_argument_takes_the_type_that_another_argument_fixes() {
    let list = Type::array(Type::func(vec![Type::I32], Type::Void));
    for body in [
        "function f(): void { const r = two(one, [one, (x) => print(`two ${x}`)]); }",
        "function f(): void { const r = twoR([one, (x) => print(`twoR ${x}`)], one); }",
        "function f(): void { const r = two<CB>(one, [one, (x) => print(`two ${x}`)]); }",
        "function f(): void { const r = twoR<CB>([one, (x) => print(`twoR ${x}`)], one); }",
    ] {
        let module = accepts(body);
        assert_eq!(initializer(&module, "r").ty, list, "{body}");
    }
    accepts("function f(): void { three(one, one, (x) => print(`t ${x}`)); }");
    // The inferred type of the other arguments, in either order: with
    // `maybe()` it is `CB | null`, which gives the parameter types of `CB`
    // (§251 rule 2).
    for body in [
        "function f(): void { three(one, maybe(), (x) => print(`t ${x}`)); }",
        "function f(): void { three(maybe(), one, (x) => print(`t ${x}`)); }",
    ] {
        accepts(body);
    }
}

/// Rule 3: a hint keeps its other uses. A literal element takes the type
/// of the first element, and the selection of §248 does not change: an
/// annotated lambda element takes the type that each element fits.
#[test]
fn a_hint_keeps_its_other_uses() {
    let module = accepts("function f(): void { const xs = [1.5 as f32, 2]; }");
    assert_eq!(
        initializer(&module, "xs").ty,
        Type::array(Type::F32),
        "a literal element takes the hint"
    );
    let module = accepts("function f(): void { const fs = [(): void => {}, one]; }");
    assert_eq!(
        initializer(&module, "fs").ty,
        Type::array(Type::func(vec![Type::I32], Type::Void))
    );
}

/// Rules 1a and 4: an operator gives a hint to its operands as it gives a
/// contextual type, so each literal operand takes the type of the first
/// element, as at the pin. The forms that the pin rejected keep their
/// S007.
#[test]
fn an_operator_gives_the_hint_to_its_literal_operands() {
    let locals = "const a: f32 = 1.5; const u: u8 = 1;";
    for (list, element) in [
        ("[a, 1 + 2]", Type::F32),
        ("[a, (1 + 2) * 3]", Type::F32),
        ("[a, 7 % 2]", Type::F32),
        ("[a, 1 > 0 ? 2 + 1 : 3]", Type::F32),
        ("[u, ~1]", Type::U8),
        ("[1 as u8, 2 * 3]", Type::U8),
    ] {
        let body = format!("function f(): void {{ {locals} const xs = {list}; }}");
        let module = accepts(&body);
        assert_eq!(
            initializer(&module, "xs").ty,
            Type::array(element),
            "{list}"
        );
    }
    for (list, message) in [
        (
            "[u, 1 << 2]",
            "implicit numeric conversion from `i32` to `u8`; spell it `as u8`",
        ),
        (
            "[u, 1 & 3]",
            "implicit numeric conversion from `i32` to `u8`; spell it `as u8`",
        ),
        (
            "[a, Math.abs(1)]",
            "implicit numeric conversion from `f64` to `f32`; spell it `as f32`",
        ),
        (
            "[a, [1, 2].length]",
            "implicit numeric conversion from `i32` to `f32`; spell it `as f32`",
        ),
    ] {
        let body = format!("function f(): void {{ {locals} const xs = {list}; }}");
        let diagnostic = rejects(&body);
        assert_eq!(diagnostic.code, RuleCode::S007, "{list}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{list}");
    }
}
