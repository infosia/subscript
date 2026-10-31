//! Equality with a value that has no absence narrows (compiler.md §219.1
//! rule 4). Each accepted form has a rejected control in the same shape.
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process. The whole file took 0.05 s in a
//! debug build.

use subscript_compiler::{check_program, divergence::Divergence, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = "class C { v: i32 = 1; }
class H {
  s: string | null = null;
  t: string = \"t\";
  c: C = new C();
  read(): i32 { return 0; }
}
let g: string | null = null;
function next(): string { return \"n\"; }
function maybe(): string | null { return null; }
";

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source)]).map(|_| ())
}

fn assert_accepted(body: &str) {
    let result = check(body);
    assert!(result.is_ok(), "{body}: {:?}", result.err());
}

/// The body is rejected with S011 and the given divergence.
fn assert_unnarrowed(body: &str, divergence: Option<Divergence>) {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    assert_eq!(errors[0].code, RuleCode::S011, "{body}");
    assert_eq!(errors[0].divergence, divergence, "{body}");
}

#[test]
fn the_operand_type_decides_and_not_its_syntax() {
    for operand in [
        "text",
        "next()",
        "\"lit\"",
        "h.t",
        "`${h.c.v}`",
        "text + \"x\"",
    ] {
        assert_accepted(&format!(
            "function f(a: string | null, text: string, h: H): i32 {{ if (a === {operand}) {{ return a.length; }} return 0; }}"
        ));
    }
    for operand in ["other", "maybe()", "h.s"] {
        assert_unnarrowed(
            &format!(
                "function f(a: string | null, other: string | null, h: H): i32 {{ if (a === {operand}) {{ return a.length; }} return 0; }}"
            ),
            None,
        );
    }
    // A null test on the operand gives it a type with no absence.
    assert_accepted(
        "function f(a: string | null, b: string | null): i32 { if (b === null) { return 0; } if (a === b) { return a.length; } return 0; }",
    );
}

#[test]
fn each_operator_and_operand_order_narrow_on_their_equal_arm() {
    for (test, equal_arm) in [
        ("a === b", true),
        ("b === a", true),
        ("a == b", true),
        ("b == a", true),
        ("a !== b", false),
        ("b !== a", false),
        ("a != b", false),
        ("b != a", false),
    ] {
        for (parameters, read) in [
            ("a: string | null, b: string", "a.length"),
            ("a: C | null, b: C", "a.v"),
        ] {
            let (equal, other) = (format!("return {read};"), "return 0;");
            let (then, otherwise) = if equal_arm {
                (equal.as_str(), other)
            } else {
                (other, equal.as_str())
            };
            assert_accepted(&format!(
                "function f({parameters}): i32 {{ if ({test}) {{ {then} }} else {{ {otherwise} }} }}"
            ));
            // Control: the other arm learns nothing.
            assert_unnarrowed(
                &format!(
                    "function f({parameters}): i32 {{ if ({test}) {{ {otherwise} }} else {{ {then} }} }}"
                ),
                None,
            );
        }
    }
}

#[test]
fn an_early_return_keeps_the_fact_and_a_join_ends_it() {
    assert_accepted(
        "function f(a: string | null): i32 { if (a !== \"x\") { return 0; } return a.length; }",
    );
    assert_unnarrowed(
        "function f(a: string | null): i32 { if (a === \"x\") { print(\"y\"); } return a.length; }",
        None,
    );
}

#[test]
fn an_undefined_result_narrows_as_a_null_result_does() {
    for (declaration, test, read) in [
        ("const r = xs.pop();", "r === \"x\"", "r.length"),
        ("const r = xs.shift();", "\"x\" == r", "r.length"),
        ("const r = m.get(\"k\");", "r === next()", "r.length"),
        (
            "const r = cs.find((c: C): boolean => true);",
            "r === cs[0]",
            "r.v",
        ),
    ] {
        let function = |test: &str| {
            format!(
                "function f(xs: string[], m: Map<string, string>, cs: C[]): i32 {{ {declaration} if ({test}) {{ return {read}; }} return 0; }}"
            )
        };
        assert_accepted(&function(test));
        // Control: a test that states nothing about `r`.
        assert_unnarrowed(&function("xs.length > 0"), None);
    }
}

#[test]
fn a_shared_location_ends_at_a_call() {
    let shared = Some(Divergence::SharedLocationNarrowing);
    for (location, parameters) in [("h.s", "h: H"), ("g", "")] {
        let read = |test: &str, body: &str| {
            format!(
                "function f({parameters}): i32 {{ if ({test}) {{ {body}return {location}.length; }} return 0; }}"
            )
        };
        assert_accepted(&read(&format!("{location} === \"x\""), ""));
        // The left operand is read before the call in the right operand.
        assert_unnarrowed(&read(&format!("{location} === next()"), ""), shared);
        // The location is read after the call in the left operand.
        assert_accepted(&read(&format!("next() === {location}"), ""));
        assert_unnarrowed(&read(&format!("{location} === \"x\""), "next(); "), shared);
    }
}

#[test]
fn the_flow_rules_of_a_null_test_apply() {
    assert_accepted(
        "function f(xs: string[]): i32 { let a: string | null = null; let n = 0; for (const x of xs) { if (a === x) { n += a.length; } a = maybe(); } return n; }",
    );
    assert_accepted(
        "function f(): i32 { let a = maybe(); while (a !== \"x\") { a = maybe(); } return a.length; }",
    );
    assert_accepted("function f(a: string | null): i32 { if (a === \"y\") { a ??= \"z\"; return a.length; } return 0; }");
    assert_accepted("function f(a: string | null): i32 { return a === \"y\" ? a.length : 0; }");
    assert_accepted(
        "function f(a: string | null): boolean { return a !== \"y\" || a.length > 0; }",
    );
    assert_accepted(
        "function f(a: string | null): i32 { if (!(a === \"y\")) { return 0; } return a.length; }",
    );
    assert_accepted(
        "function f(): i32 { let a: string | null = null; if ((a = maybe()) === \"x\") { return a.length; } return 0; }",
    );
    // The right operand is read last, so its store does not end its own fact.
    assert_accepted(
        "function f(): i32 { let a: string | null = null; if (\"x\" === (a = maybe())) { return a.length; } return 0; }",
    );
    // A store of a nullable value ends the fact.
    assert_unnarrowed(
        "function f(a: string | null): i32 { if (a === \"y\") { a = maybe(); return a.length; } return 0; }",
        None,
    );
}

#[test]
fn a_type_parameter_operand_narrows_only_with_no_absence() {
    assert_accepted(
        "function f<T extends string>(a: string | null, b: T): i32 { if (a === b) { return a.length; } return 0; }",
    );
    assert_accepted(
        "function f<T extends C>(a: C | null, b: T): i32 { if (a === b) { return a.v; } return 0; }",
    );
    let errors = check(
        "function f<T>(a: T | null, b: T): T | null { if (a === b) { const r: T = a; return r; } return null; }",
    )
    .expect_err("an unconstrained parameter can be nullable");
    assert_eq!(errors[0].code, RuleCode::S100, "{errors:?}");
}

#[test]
fn a_generic_union_narrows_only_if_each_member_has_no_absence() {
    assert_accepted(
        "function f<T extends C, U extends H>(a: C | null, t: T, u: U, c: boolean): i32 { if (a === (c ? t : u)) { return a.v; } return 0; }",
    );
    assert_unnarrowed(
        "function f<T extends C>(a: C | null, b: T, c: boolean): i32 { if (a === (c ? b : null)) { return a.v; } return 0; }",
        None,
    );
    let errors = check(
        "function f<T>(a: T | null, b: T, c: boolean): T | null { if (a === (c ? b : null)) { const r: T = a; return r; } return null; }",
    )
    .expect_err("a union with a null member can be absent");
    assert_eq!(errors[0].code, RuleCode::S100, "{errors:?}");
}

#[test]
fn a_poison_operand_gives_no_narrowing() {
    let errors = check(
        "function f(a: string | null): i32 { if (a === (nosuch + 1)) { return a.length; } return 0; }",
    )
    .expect_err("the operand names no declaration");
    let codes: Vec<RuleCode> = errors.iter().map(|error| error.code).collect();
    assert_eq!(codes, [RuleCode::S016, RuleCode::S011], "{errors:?}");
}

#[test]
fn a_class_compares_with_its_nullable_form_and_not_with_a_second_one() {
    assert_accepted("function f(a: C | null, b: C): boolean { return a === b; }");
    assert_accepted("function f(a: C | null, b: C): boolean { return b !== a; }");
    for rejected in [
        "function f(a: C | null, b: C | null): boolean { return a === b; }",
        "function f(a: C | null, b: H): boolean { return a === b; }",
    ] {
        let errors = check(rejected).expect_err(rejected);
        assert_eq!(errors[0].code, RuleCode::S100, "{errors:?}");
    }
}
