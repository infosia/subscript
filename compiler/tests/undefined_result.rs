//! compiler.md §195: each §195.8 row that no reject entry pins, the
//! result type of `??`, the narrowing of the `undefined` spelling, and a
//! `let` reassignment.

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, render_diagnostics, Diagnostic, RuleCode, SourceFile};

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
    // §195.1 rule 2: a `T | undefined` result does not convert to `T | null`.
    assert_row(
        "export function main(): void { const b: Box | null = maybe() ?? pick(); print(`${b == null}`); }",
        RuleCode::S012,
        "`T | undefined` and `T | null` do not convert to each other: the initializer expects `Box | null`, got `Box | undefined`",
    );
    // Right operand `null`: the result is `Box | null`, for each left spelling.
    assert_accepted(
        "export function main(): void { const b: Box | null = pick() ?? null; const c: Box | null = maybe() ?? null; print(`${b === null} ${c === null}`); }",
    );
    assert_accepted(
        "export function main(): void { const b = pick() ?? null; print(`${b === null}`); }",
    );
    assert_row(
        "export function main(): void { const b = pick() ?? null; print(`${b === undefined}`); }",
        RuleCode::S012,
        "`===` compares a `null` and an `undefined` absence; use `==`",
    );
}

const CONVERSION: &str = "`T | undefined` and `T | null` do not convert to each other";

/// compiler.md §195.11 rule 4: a conversion between the spellings is its
/// own S012, in each direction and at each conversion site.
#[test]
fn a_conversion_between_spellings_is_s012() {
    for body in [
        "const u = pick(); const b: Box | null = u;",
        "let n: Box | null = null; n = pick();",
        "const f = (x: Box | null): void => {}; f(pick());",
    ] {
        assert_row(
            &format!("export function main(): void {{ {body} }}"),
            RuleCode::S012,
            CONVERSION,
        );
    }
    assert_row(
        "function f(): Box | undefined { return maybe(); }",
        RuleCode::S012,
        CONVERSION,
    );
    // Control: a `T` converts to both spellings.
    assert_accepted(
        "function f(): Box | undefined { return new Box(); }
function g(): Box | null { return new Box(); }",
    );
}

/// compiler.md §195.11 acceptance 3: `tsc` rejects the conversion, so the
/// diagnostic renders no "TypeScript accepts" block; the cross-spelling
/// `===` of §195.3 keeps its block.
#[test]
fn the_conversion_diagnostic_renders_no_typescript_block() {
    let render = |body: &str| {
        let source = format!("{PRELUDE}{body}\n");
        let files = [SourceFile::entry("main.ts", source)];
        let diagnostics = check_program(&files).expect_err("the source is rejected");
        assert_eq!(diagnostics[0].code, RuleCode::S012, "{body}");
        render_diagnostics(&files, &diagnostics)
    };
    let conversion = render(
        "export function main(): void { const u = pick(); const b: Box | null = u; print(`${b === null}`); }",
    );
    assert!(conversion.contains(CONVERSION), "{conversion}");
    assert!(
        !conversion.contains("= TypeScript accepts:"),
        "{conversion}"
    );
    // Control: the same pair under `===` renders the block.
    let comparison = render(
        "export function main(): void { const u = pick(); const n = maybe(); print(`${u === n}`); }",
    );
    assert!(comparison.contains(CROSS_VALUES), "{comparison}");
    assert!(comparison.contains("= TypeScript accepts:"), "{comparison}");
}

const CROSS_LITERAL: &str = "compares a `null` and an `undefined` absence; use `";
const CROSS_VALUES: &str =
    "`===` compares a `null` and an `undefined` absence; two absent values are not equal under JavaScript";

#[test]
fn the_cross_spelling_message_names_a_replacement_only_for_a_literal_operand() {
    // §195.3 rule 2: the `null` literal or the `undefined` token names `==`.
    for (body, replacement) in [
        ("const b = pick() === null;", "use `==`"),
        ("const b = null !== pick();", "use `!=`"),
        ("const b = maybe() === undefined;", "use `==`"),
        ("const b = undefined !== maybe();", "use `!=`"),
    ] {
        let diagnostic = assert_row(
            &format!("export function main(): void {{ {body} }}"),
            RuleCode::S012,
            CROSS_LITERAL,
        );
        assert!(
            diagnostic.message.ends_with(replacement),
            "{body}: {}",
            diagnostic.message
        );
    }
    // Two values: the message names no replacement.
    for body in [
        "const b = pick() === maybe();",
        "const b = maybe() !== pick();",
    ] {
        let diagnostic = first_error(&format!("export function main(): void {{ {body} }}"));
        assert_eq!(diagnostic.code, RuleCode::S012, "{body}");
        assert!(
            diagnostic
                .message
                .ends_with("two absent values are not equal under JavaScript"),
            "{body}: {}",
            diagnostic.message
        );
        assert!(!diagnostic.message.contains("use `"), "{body}");
    }
    assert_row(
        "export function main(): void { const b = pick() === maybe(); }",
        RuleCode::S012,
        CROSS_VALUES,
    );
}

#[test]
fn a_rejected_cross_spelling_test_still_narrows() {
    // §195.3 rule 4: after the S012, the comparison is an absence test, so
    // the member read in the branch reports nothing more.
    for test in [
        "const u = pick(); if (u !== null) { print(`${u.v}`); }",
        "const n = maybe(); if (n !== undefined) { print(`${n.v}`); }",
        "const u = pick(); if (null === u) { return; } print(`${u.v}`);",
    ] {
        let diagnostics = check(&format!("export function main(): void {{ {test} }}"))
            .expect_err("a cross-spelling comparison is rejected");
        assert_eq!(diagnostics.len(), 1, "{test}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S012, "{test}");
        assert!(diagnostics[0].message.contains(CROSS_LITERAL), "{test}");
    }
    // Control: without the comparison, the member read is rejected.
    let diagnostics = check("export function main(): void { const u = pick(); print(`${u.v}`); }")
        .expect_err("an unnarrowed member read is rejected");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
}

#[test]
fn a_conditional_does_not_join_the_null_literal_with_the_undefined_spelling() {
    const BOTH: &str = "`T | null | undefined` has no representation";
    for join in [
        "c ? null : pick()",
        "c ? pick() : null",
        "c ? pick() : (c ? null : null)",
        "c ? new Box() : (c ? pick() : null)",
        "c ? null : new Map<i32, Box>().get(1)",
    ] {
        assert_row(
            &format!(
                "export function main(): void {{ const c = pick() === undefined; const x = {join}; print(`${{x == null}}`); }}"
            ),
            RuleCode::S011,
            BOTH,
        );
    }
    // Control: the `null` literal joins the `null` spelling and a `T`.
    assert_accepted(
        "export function main(): void { const c = true; const x = c ? null : maybe(); const y = c ? new Box() : null; print(`${x === null} ${y === null}`); }",
    );
    // Control: a `T` joins the `undefined` spelling.
    assert_accepted(
        "export function main(): void { const c = true; const x = c ? new Box() : pick(); print(`${x === undefined}`); }",
    );
}

/// compiler.md §195.1 rule 3: a `T | null` value branch has the `null`
/// spelling, as the `null` literal has.
#[test]
fn a_conditional_does_not_join_a_null_value_with_the_undefined_spelling() {
    for join in ["c ? pick() : maybe()", "c ? maybe() : pick()"] {
        assert_row(
            &format!(
                "export function main(): void {{ const c = true; const x = {join}; print(`${{x == null}}`); }}"
            ),
            RuleCode::S011,
            "`T | null | undefined` has no representation",
        );
    }
    // Control: two branches of one spelling join.
    assert_accepted(
        "export function main(): void { const c = true; const x = c ? maybe() : maybe(); const y = c ? pick() : pick(); print(`${x === null} ${y === undefined}`); }",
    );
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
    // `tsc` gives TS2322 for `null`, so the diagnostic carries no block.
    let diagnostic = assert_row(
        "export function main(): void { let c = pick(); c = null; }",
        RuleCode::S100,
        "type mismatch: the assignment expects `Box | undefined`, got `null`",
    );
    assert_eq!(diagnostic.divergence, None);
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

const BOTH: &str = "`T | null | undefined` has no representation";

/// compiler.md §195.11 rule 5: a generic `??` chain keeps one spelling at
/// each step.
#[test]
fn a_generic_nullish_chain_keeps_one_spelling() {
    let class = "class Layers<V> {\n\
        \x20 a: Map<string, V> = new Map<string, V>();\n\
        \x20 b: Map<string, V> = new Map<string, V>();\n\
        \x20 d: V | null = null;\n";
    for member in [
        "look(k: string): V | null { return this.a.get(k) ?? this.b.get(k) ?? this.d; }",
        "either(k: string): boolean { const x = this.a.get(k) ?? this.b.get(k); return x !== undefined; }",
        "first(k: string): boolean { const x = this.d ?? this.a.get(k); return x !== undefined; }",
        "slot(k: string): V | null { return this.a.get(k) ?? null; }",
        "flip(k: string): V | null { const x = this.a.get(k) ?? null; return x; }",
    ] {
        assert_accepted(&format!(
            "{class}  {member}\n}}\nexport function main(): void {{ const l = new Layers<Box>(); }}"
        ));
    }
    // Control: the spelling of each step is checked.
    assert_row(
        &format!(
            "{class}  either(k: string): boolean {{ const x = this.a.get(k) ?? this.b.get(k); return x !== null; }}\n}}\nexport function main(): void {{ const l = new Layers<Box>(); }}"
        ),
        RuleCode::S012,
        "`!==` compares a `null` and an `undefined` absence",
    );
}

/// compiler.md §195.11 rule 6: a conditional with one branch of each
/// spelling is S011 with a contextual type too, also in a generic body.
#[test]
fn a_cross_spelling_conditional_is_s011_with_a_contextual_type() {
    assert_row(
        "export function main(): void { const c = true; const x: Box | null = c ? pick() : maybe(); print(`${x === null}`); }",
        RuleCode::S011,
        BOTH,
    );
    let generic = |body: &str| {
        format!(
            "class L<T> {{\n  m: Map<string, T> = new Map<string, T>();\n  n: T | null = null;\n  c0(): boolean {{ return true; }}\n  {body}\n}}\nexport function main(): void {{ const l = new L<Box>(); }}"
        )
    };
    assert_row(
        &generic(
            "get(k: string): T | null { const a = this.m.get(k); return this.c0() ? a : this.n; }",
        ),
        RuleCode::S011,
        BOTH,
    );
    // Control: two branches of the `null` spelling join under the context.
    assert_accepted(&generic(
        "get(k: string): T | null { return this.c0() ? this.n : null; }",
    ));
    assert_accepted(
        "export function main(): void { const c = true; const x: Box | null = c ? maybe() : null; print(`${x === null}`); }",
    );
}

/// compiler.md §195.11 acceptance 6: `x ?? null` in the type-parameter
/// path, and after an optional chain with a non-nullable value.
#[test]
fn nullish_null_in_the_generic_and_optional_chain_paths() {
    // Type-parameter path: the result is `T | null`.
    assert_accepted(
        "function slot<T>(m: Map<string, T>, k: string): T | null { const x = m.get(k) ?? null; return x; }\n\
         export function main(): void { const m = new Map<string, Box>(); print(`${slot(m, \"a\") === null}`); }",
    );
    assert_row(
        "function slot<T>(m: Map<string, T>, k: string): boolean { const x = m.get(k) ?? null; return x === undefined; }\n\
         export function main(): void { const m = new Map<string, Box>(); print(`${slot(m, \"a\")}`); }",
        RuleCode::S012,
        "`===` compares a `null` and an `undefined` absence",
    );
    // Optional chain with a non-nullable value: `h?.kid ?? null` is `Leaf | null`.
    let tree = "class Leaf { v: i32 = 1; }\nclass Tree { kid: Leaf = new Leaf(); }\n";
    assert_accepted(&format!(
        "{tree}export function main(): void {{ const h: Tree | null = null; const x: Leaf | null = h?.kid ?? null; const y = h?.kid ?? null; print(`${{x === null}} ${{y === null}}`); }}"
    ));
    // Control: the result has the `null` spelling.
    assert_row(
        &format!(
            "{tree}export function main(): void {{ const h: Tree | null = null; const y = h?.kid ?? null; print(`${{y === undefined}}`); }}"
        ),
        RuleCode::S012,
        "`===` compares a `null` and an `undefined` absence",
    );
}
