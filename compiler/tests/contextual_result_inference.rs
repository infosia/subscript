//! A generic call infers from its contextual result type (`compiler.md` §212).
//!
//! Cost: each test runs the checker on one short program, with no code
//! generation and no external process. The whole file took 0.07 s in a
//! debug build.

use subscript_compiler::{check_program, RuleCode, SourceFile};

const PRELUDE: &str = r#"
class P { x: i32 = 1; }
class Q { y: i32 = 2; }
class Buf<T> { items: T[] = []; }
class Two<A, B> { a: A; b: B; constructor(a: A, b: B) { this.a = a; this.b = b; } }
function empty<T>(): Buf<T> { return new Buf<T>(); }
function wrap<T>(value: T): Buf<T> {
    const b = new Buf<T>();
    b.items.push(value);
    return b;
}
function id<T>(value: T): T { return value; }
function zero<T>(): T { throw new Error("zero"); }
function fromNullable<T>(x: T | null): T[] { return []; }
function fromPlain<T>(x: T): T[] { return []; }
function lead<T>(x: T | null, b: Buf<T>): T[] { return []; }
function nullBuf<T>(x: Buf<T> | null): Buf<T> { return new Buf<T>(); }
function trail<T>(b: Buf<T>, x: T | null): Buf<T> { return b; }
function run(f: () => void): void { f(); }
function same<T>(): Two<T, T> { throw new Error("same"); }
class Factory { make<T>(): Buf<T> { return new Buf<T>(); } }
"#;

fn program(main: &str) -> String {
    format!("{PRELUDE}\nexport function main(): void {{\n{main}\n}}\n")
}

fn accept(main: &str) {
    check_program(&[SourceFile::new("test.ts", program(main).as_str())])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
}

fn reject(main: &str, code: RuleCode, message: &str) {
    let source = program(main);
    let errors =
        check_program(&[SourceFile::new("test.ts", source.as_str())]).expect_err("must reject");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, code, "{errors:?}");
    assert_eq!(errors[0].message, message);
}

fn no_candidate(parameter: &str, name: &str) -> String {
    format!("cannot infer type parameter `{parameter}` of `{name}`: no candidate; use explicit type arguments")
}

/// Rule 2: the context takes the place of a literal default.
#[test]
fn a_contextual_candidate_precedes_a_literal_default() {
    accept("const b: Buf<i64> = wrap(7);");
    accept("const value: i64 = id(7);");
    reject(
        "const b = wrap(7); const c: Buf<i64> = b;",
        RuleCode::S005,
        "nominal types are not interchangeable: the initializer expects `Buf<i64>`, got `Buf<i32>`",
    );
}

/// Rule 2: the nested call has the context of its parameter type.
#[test]
fn a_nested_generic_call_takes_the_replaced_parameter_type() {
    accept("const b: Buf<Buf<P>> = wrap(empty());");
    reject(
        "const b = wrap(empty());",
        RuleCode::S100,
        &no_candidate("T", "empty"),
    );
}

/// Rules 2 and 4: a non-literal argument candidate stays, and the context
/// reports the mismatch.
#[test]
fn an_argument_candidate_precedes_the_context() {
    accept("const b: Buf<P> = wrap(new P());");
    reject(
        "const b: Buf<P> = wrap(new Q());",
        RuleCode::S005,
        "nominal types are not interchangeable: the initializer expects `Buf<P>`, got `Buf<Q>`",
    );
}

/// Rule 3: a `null` argument that mentions the parameter stops the context.
#[test]
fn a_null_argument_keeps_the_no_candidate_diagnostic() {
    reject(
        "const x: P[] = fromNullable(null);",
        RuleCode::S100,
        &no_candidate("T", "fromNullable"),
    );
    reject(
        "const x: P[] = fromPlain(null);",
        RuleCode::S100,
        &no_candidate("T", "fromPlain"),
    );
    reject(
        "const x: P[] = lead(null, empty());",
        RuleCode::S100,
        &no_candidate("T", "empty"),
    );
    // The control: a non-null argument of the same parameter is accepted.
    accept("const p: P | null = new P(); const x: P[] = fromNullable(p);");
}

/// Rule 3: a `null` argument for another shape gives no candidate, so the
/// context decides the parameter.
#[test]
fn a_null_argument_of_another_shape_keeps_the_context() {
    accept("const b: Buf<P> = nullBuf(null);");
    // The control: the same call with no context.
    reject(
        "const b = nullBuf(null);",
        RuleCode::S100,
        &no_candidate("T", "nullBuf"),
    );
}

/// Rule 3: an argument that the checker reads before the `null` keeps its
/// context.
#[test]
fn an_argument_before_the_null_keeps_its_context() {
    accept("const b: Buf<P> = trail(empty(), null);");
    // The control: the same callee with no context.
    reject(
        "const b = trail(empty(), null);",
        RuleCode::S100,
        &no_candidate("T", "empty"),
    );
}

/// Rule 5: a call with no context keeps the diagnostic.
#[test]
fn a_call_with_no_context_keeps_the_diagnostic() {
    reject(
        "const b = empty();",
        RuleCode::S100,
        &no_candidate("T", "empty"),
    );
    reject(
        "print(`${new Factory().make().items.length}`);",
        RuleCode::S100,
        &no_candidate("T", "Factory.make"),
    );
    // The control: the same calls with a context.
    accept("const b: Buf<P> = empty(); const c: Buf<P> = new Factory().make();");
}

/// Rule 1: a shape that does not match gives no candidate and no new text.
#[test]
fn a_context_of_another_shape_gives_no_candidate() {
    reject(
        "const x: i32 = empty();",
        RuleCode::S100,
        &no_candidate("T", "empty"),
    );
    reject(
        "const x: P[] = empty();",
        RuleCode::S100,
        &no_candidate("T", "empty"),
    );
}

/// Rule 1: the match goes through `C | null`, and a bare result takes the
/// whole contextual type.
#[test]
fn the_match_goes_through_a_nullable_context() {
    accept("const b: Buf<P> | null = empty();");
    accept("const p: P | null = zero();");
}

/// Rule 1: the first match of a repeated parameter decides it, and the
/// context reports the rest.
#[test]
fn a_repeated_parameter_takes_the_first_match() {
    reject(
        "const t: Two<P, Q> = same();",
        RuleCode::S005,
        "nominal types are not interchangeable: the initializer expects `Two<P, Q>`, got `Two<P, P>`",
    );
}

/// Rule 1: the target type of a plain assignment is a context. A compound
/// assignment, an operand, and an index have no contextual type.
#[test]
fn only_a_contextual_position_gives_a_candidate() {
    accept("let b: Buf<P>; b = empty(); print(`${b.items.length}`);");
    reject(
        "const x: i32 = -zero();",
        RuleCode::S100,
        &no_candidate("T", "zero"),
    );
    reject(
        "const x: i32 = 1 + zero();",
        RuleCode::S100,
        &no_candidate("T", "zero"),
    );
    let compound = program("let x: i32 = 1; x += zero();");
    let errors =
        check_program(&[SourceFile::new("test.ts", compound.as_str())]).expect_err("must reject");
    assert_eq!(errors[0].message, no_candidate("T", "zero"));
    reject(
        "const xs: i32[] = [1]; print(`${xs[zero()]}`);",
        RuleCode::S100,
        &no_candidate("T", "zero"),
    );
    // The control: the same callee in a contextual position.
    accept("const x: i32 = zero();");
}

/// Rule 1a: each hint position gives no candidate, also through a
/// conditional branch. A reject here fails if its mark is removed.
#[test]
fn a_literal_hint_gives_no_candidate() {
    let no_zero = no_candidate("T", "zero");
    reject("const a: i32 = ~zero();", RuleCode::S100, &no_zero);
    reject(
        "const a: i32 = 1; switch (a) { case zero(): break; default: break; }",
        RuleCode::S100,
        &no_zero,
    );
    reject(
        "const c: boolean = true; const x: i32 = -(c ? zero() : 1);",
        RuleCode::S100,
        &no_zero,
    );
    // The control: the same conditional in a contextual position.
    accept("const c: boolean = true; const x: i32 = c ? zero() : 1;");
}

/// Rule 1a: an earlier element of an array literal is a hint.
#[test]
fn an_earlier_array_element_gives_no_candidate() {
    let no_zero = no_candidate("T", "zero");
    reject("const xs = [new P(), zero()];", RuleCode::S100, &no_zero);
    reject("const xs = [1, zero()];", RuleCode::S100, &no_zero);
    reject(
        "const ys: i32[] = [1]; const xs = [...ys, zero()];",
        RuleCode::S100,
        &no_zero,
    );
    reject("const xs = [[1], [zero()]];", RuleCode::S100, &no_zero);
    // The control: the context of the literal gives the candidate.
    accept("const xs: P[] = [new P(), zero()];");
    accept("const ys: i32[] = [1]; const xs: i32[] = [...ys, zero()];");
    // The hint still types a literal.
    accept("const a: i64 = 1; const xs = [a, 2];");
}

/// Rule 1a: the right operand of `??`, a conditional branch, and the
/// operand of `satisfies` keep the context.
#[test]
fn a_contextual_operand_keeps_the_context() {
    accept("const a: Buf<P> | null = null; const b = a ?? empty();");
    accept("const b: Buf<P> = empty() satisfies Buf<P>;");
    accept("const c: boolean = true; const b: Buf<P> = c ? empty() : new Buf<P>();");
}

/// §212.3 item 6a: the right operand of `??` has no context in a generic
/// body, so a call there has no candidate.
#[test]
fn a_nullish_right_operand_in_a_generic_body_has_no_context() {
    let source = format!(
        "{PRELUDE}
function f<U>(a: Buf<U> | null): Buf<U> {{ return a ?? empty(); }}
export function main(): void {{}}
"
    );
    let errors =
        check_program(&[SourceFile::new("test.ts", source.as_str())]).expect_err("must reject");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100, "{errors:?}");
    assert_eq!(errors[0].message, no_candidate("T", "empty"));
    // The control: a literal default decides the same position.
    let source = format!(
        "{PRELUDE}
function g<U>(a: U | null): void {{ const r = a ?? id(1); }}
export function main(): void {{}}
"
    );
    check_program(&[SourceFile::new("test.ts", source.as_str())])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
}

/// Rule 1: `void` gives no candidate, so the call keeps the text of its
/// literal default.
#[test]
fn void_gives_no_candidate() {
    let body = "type mismatch: the lambda body expects `void`, got `i32`";
    reject("run(() => id(1));", RuleCode::S100, body);
    reject(
        "const xs: i32[] = [1]; xs.forEach((x: i32) => id(x));",
        RuleCode::S100,
        body,
    );
    reject(
        "const f: () => void = () => zero();",
        RuleCode::S100,
        &no_candidate("T", "zero"),
    );
    // The control: a call with no result takes the same context.
    accept("run(() => { id(1); });");
}

/// Rule 1: the context of a call does not reach a call that it holds.
#[test]
fn a_contextual_candidate_stays_with_its_call() {
    reject(
        "const b: Buf<P> = id(zero<Factory>()).make();
         const c: Buf<P> = id(zero()).make();",
        RuleCode::S100,
        &no_candidate("T", "zero"),
    );
}

/// An enclosing type parameter is a legal candidate, and the opaque body
/// and each instance agree.
#[test]
fn an_enclosing_type_parameter_is_a_candidate() {
    let source = format!(
        "{PRELUDE}
function forward<U>(): Buf<U> {{ return empty(); }}
function pass<U>(value: U): U {{ return zero(); }}
export function main(): void {{
    const a: Buf<P> = forward<P>();
    const b: Buf<Q> = forward<Q>();
    const c: P = pass(new P());
    print(`${{a.items.length}} ${{b.items.length}} ${{c.x}}`);
}}
"
    );
    check_program(&[SourceFile::new("test.ts", source.as_str())])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
}

/// A partial list stays rejected, as `tsc` TS2558 does.
#[test]
fn a_partial_type_argument_list_stays_rejected() {
    let source = program("const t: Two<P, Q> = pair<P>(); print(`${t.a.x}`);").replace(
        "export function main",
        "function pair<A, B>(): Two<A, B> { throw new Error(\"pair\"); }\nexport function main",
    );
    let errors =
        check_program(&[SourceFile::new("test.ts", source.as_str())]).expect_err("must reject");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].message.contains("expects 2 type argument"),
        "{errors:?}"
    );
}
