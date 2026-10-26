//! compiler.md §195: each §195.8 row that no reject entry pins, the
//! result type of `??`, the narrowing of the `undefined` spelling, and a
//! `let` reassignment.

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const PRELUDE: &str = "class Box { v: i32 = 1; }\n\
    function pick(): Box | undefined { return undefined; }\n\
    function maybe(): Box | null { return null; }\n";

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{PRELUDE}{body}\n");
    check_program(&[SourceFile::entry("main.ts", source)]).map(|_| ())
}

fn first_error(body: &str) -> Diagnostic {
    match check(body) {
        Ok(()) => panic!("accepted: {body}"),
        Err(diagnostics) => diagnostics[0].clone(),
    }
}

fn assert_row(body: &str, code: RuleCode, message: &str) -> Diagnostic {
    let diagnostic = first_error(body);
    assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
    assert!(
        diagnostic.message.contains(message),
        "{body}: `{}` does not contain `{message}`",
        diagnostic.message
    );
    diagnostic
}

fn assert_accepted(body: &str) {
    if let Err(diagnostics) = check(body) {
        panic!("{body}: {diagnostics:?}");
    }
}

const POSITION: &str =
    "`T | undefined` is legal only as the result type of a function declaration or a class method";
const TOKEN: &str = "`undefined` is legal only in `return undefined;` and in an absence comparison";

#[test]
fn return_undefined_in_a_null_result_is_the_token_row_without_a_block() {
    let diagnostic = assert_row(
        "function f(): Box | null { return undefined; }",
        RuleCode::S012,
        TOKEN,
    );
    // `tsc` gives TS2322 here, so the diagnostic carries no block.
    assert_eq!(diagnostic.divergence, None);
    assert_accepted("function f(): Box | undefined { return undefined; }");
}

#[test]
fn a_token_pair_and_a_scalar_operand_are_the_token_row() {
    for body in [
        "export function main(): void { const b = undefined === undefined; }",
        "export function main(): void { const b = undefined === 1; }",
        "export function main(): void { const x: i32 = 1; const b = x !== undefined; }",
    ] {
        assert_row(body, RuleCode::S012, TOKEN);
    }
    // Control: the token against a nullable value is an absence comparison.
    assert_accepted("export function main(): void { const b = pick() === undefined; }");
}

#[test]
fn a_nullish_undefined_operand_is_the_token_row() {
    assert_row(
        "export function main(): void { const b = pick() ?? undefined; }",
        RuleCode::S012,
        TOKEN,
    );
    assert_accepted("export function main(): void { const b = pick() ?? new Box(); }");
}

#[test]
fn null_and_undefined_in_one_union_is_s011() {
    let diagnostic = assert_row(
        "function f(): Box | null | undefined { return null; }",
        RuleCode::S011,
        "`T | null | undefined` has no representation",
    );
    assert_eq!(diagnostic.divergence, Some(Divergence::UndefinedNullUnion));
}

#[test]
fn each_other_annotation_position_is_the_position_row() {
    for body in [
        "type MaybeBox = Box | undefined;",
        "function apply(f: () => Box | undefined): void {}",
        "async function f(): Promise<Box | undefined> { return undefined; }",
        "function* f(): Generator<Box | undefined> { yield undefined; }",
        "function f(): (Box | undefined)[] { return []; }",
        "class Holder { get item(): Box | undefined { return undefined; } }",
    ] {
        let diagnostic = assert_row(body, RuleCode::S012, POSITION);
        assert_eq!(
            diagnostic.divergence,
            Some(Divergence::UndefinedResultPosition),
            "{body}"
        );
    }
    // Control: a method result and a generic method result are legal.
    assert_accepted("class Holder { item(): Box | undefined { return undefined; } }");
    assert_accepted(
        "class Store<V> { items: V[] = []; first(): V | undefined { if (this.items.length === 0) { return undefined; } return this.items[0]; } }\n\
         export function main(): void { const s = new Store<Box>(); print(`${s.first() === undefined}`); }",
    );
}

#[test]
fn a_scalar_type_argument_of_an_undefined_result_is_s011() {
    assert_row(
        "class Store<V> { items: V[] = []; first(): V | undefined { if (this.items.length === 0) { return undefined; } return this.items[0]; } }\n\
         export function main(): void { const s = new Store<i32>(); s.first(); }",
        RuleCode::S011,
        "`i32 | undefined` is not a reference type union",
    );
}

#[test]
fn find_on_a_nullable_element_is_s014() {
    for member in ["find", "findLast"] {
        let diagnostic = assert_row(
            &format!(
                "export function main(): void {{ const xs: (Box | null)[] = [null]; xs.{member}((b: Box | null): boolean => b === null); }}"
            ),
            RuleCode::S014,
            "the result needs both `null` and `undefined`",
        );
        assert_eq!(
            diagnostic.divergence,
            Some(Divergence::NullableElementSearch)
        );
    }
    // Control: a reference element has an `undefined` miss.
    assert_accepted(
        "export function main(): void { const xs: Box[] = [new Box()]; print(`${xs.find((b: Box): boolean => b.v === 2) === undefined}`); }",
    );
}

/// §195.4 rule 1: the result has the type of a nullable right operand,
/// with its spelling; else it is `T`.
#[test]
fn nullish_result_types_follow_the_right_operand() {
    // Right operand `T`: the result is `Box`.
    assert_accepted(
        "export function main(): void { const b: Box = pick() ?? new Box(); print(`${b.v}`); }",
    );
    // Right operand `T | null`: the result is `Box | null`.
    assert_accepted(
        "export function main(): void { const b: Box | null = pick() ?? maybe(); print(`${b === null}`); }",
    );
    assert_row(
        "export function main(): void { const b = pick() ?? maybe(); print(`${b === undefined}`); }",
        RuleCode::S012,
        "`===` compares a `null` and an `undefined` absence; use `==`",
    );
    // Right operand `T | undefined`: the result is `Box | undefined`.
    assert_accepted(
        "export function main(): void { const b = maybe() ?? pick(); print(`${b === undefined}`); }",
    );
    assert_row(
        "export function main(): void { const b = maybe() ?? pick(); print(`${b === null}`); }",
        RuleCode::S012,
        "`===` compares a `null` and an `undefined` absence; use `==`",
    );
    let mismatch = first_error(
        "export function main(): void { const b: Box | null = maybe() ?? pick(); print(`${b == null}`); }",
    );
    assert_ne!(mismatch.code, RuleCode::S012, "{mismatch:?}");
}

#[test]
fn an_absence_test_on_the_undefined_spelling_narrows() {
    for test in [
        "if (a === undefined) { return; }",
        "if (a == null) { return; }",
        "if (a == undefined) { return; }",
    ] {
        assert_accepted(&format!(
            "export function main(): void {{ const a = pick(); {test} print(`${{a.v}}`); }}"
        ));
    }
    for test in [
        "if (a !== undefined) { print(`${a.v}`); }",
        "if (a != null) { print(`${a.v}`); }",
    ] {
        assert_accepted(&format!(
            "export function main(): void {{ const a = pick(); {test} }}"
        ));
    }
    // A local that an assignment narrows compares through its storage type.
    assert_accepted(
        "export function main(): void { let c = pick(); c = new Box(); print(`${c === undefined} ${c.v}`); }",
    );
    // Control: without the test, the member read is rejected.
    assert!(check("export function main(): void { const a = pick(); print(`${a.v}`); }").is_err());
}

#[test]
fn a_let_local_takes_a_value_of_the_same_spelling() {
    assert_accepted(
        "export function main(): void { let c = pick(); c = pick(); print(`${c === undefined}`); }",
    );
    // Control: neither `null` nor a `T | null` value converts to the spelling.
    assert!(check("export function main(): void { let c = pick(); c = null; }").is_err());
    assert!(check("export function main(): void { let c = pick(); c = maybe(); }").is_err());
}

#[test]
fn an_inferred_type_argument_and_a_closure_result_are_the_nested_row() {
    const NESTED: &str = "the `undefined` spelling is legal only as the whole type of a value";
    for body in [
        "function same<T>(x: T): T { return x; }\nexport function main(): void { const b = same(pick()); }",
        "export function main(): void { const f = (): boolean => pick() === undefined; const g = () => pick(); }",
    ] {
        let diagnostic = assert_row(body, RuleCode::S012, NESTED);
        assert_eq!(
            diagnostic.divergence,
            Some(Divergence::UndefinedSpellingOutsideValue),
            "{body}"
        );
    }
}
