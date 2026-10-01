//! compiler.md §144: loose and strict equality use the same operand rules.

use super::*;

#[test]
fn loose_equality_matches_strict_operand_pair_verdicts_and_codes() {
    // These pairs cover the identity, Date, nullable, scalar, and generic tests.
    let cases = [
        ("i32", "(a: i32, b: i32)", "a", "b", None),
        ("f64", "(a: f64, b: f64)", "a", "b", None),
        ("string", "(a: string, b: string)", "a", "b", None),
        ("boolean", "(a: boolean, b: boolean)", "a", "b", None),
        ("enum", "(a: Kind, b: Kind)", "a", "b", None),
        ("class", "(a: Box, b: Box)", "a", "b", None),
        ("alias", "(a: Label, b: Label)", "a", "b", None),
        ("alias-literal", "(a: Label)", "a", "\"a\"", None),
        ("nullable", "(a: Box | null)", "a", "null", None),
        ("null-first", "(a: Box | null)", "null", "a", None),
        ("null", "()", "null", "null", None),
        (
            "nullable-function",
            "(a: (() => i32) | null)",
            "a",
            "null",
            None,
        ),
        (
            "array",
            "(a: i32[], b: i32[])",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        (
            "regexp",
            "(a: RegExp, b: RegExp)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        ("date", "(a: Date, b: Date)", "a", "b", Some(RuleCode::S014)),
        (
            "mixed-numeric",
            "(a: i32, b: f64)",
            "a",
            "b",
            Some(RuleCode::S007),
        ),
        (
            "number-string",
            "(a: i32, b: string)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        (
            "number-boolean",
            "(a: i32, b: boolean)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        (
            "class-string",
            "(a: Box, b: string)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        ("generic-same", "<T>(a: T, b: T)", "a", "b", None),
        (
            "generic-linked",
            "<U, T extends U>(a: T, b: U)",
            "a",
            "b",
            None,
        ),
        (
            "generic-distinct",
            "<T, U>(a: T, b: U)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        (
            "generic-number",
            "<T>(a: T)",
            "a",
            "1",
            Some(RuleCode::S100),
        ),
        (
            "constrained-distinct",
            "<T extends i32, U extends i32>(a: T, b: U)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
        (
            "generic-class",
            "<T extends Box>(a: T, b: T)",
            "a",
            "b",
            None,
        ),
        (
            "nullable-pair",
            "(a: Box | null, b: Box | null)",
            "a",
            "b",
            Some(RuleCode::S100),
        ),
    ];
    let declarations = "class Box { value: i32 = 1; }\n\
                        enum Kind { First, Second }\n\
                        type Label = \"a\" | \"b\";\n";
    for (name, parameters, left, right, expected_code) in cases {
        for (strict, loose) in [("===", "=="), ("!==", "!=")] {
            let invocation = match name {
                "generic-same" => "compare<i32>(1, 2);",
                "generic-linked" => "compare<Box, Box>(new Box(), new Box());",
                "generic-distinct" => "compare<i32, i32>(1, 2);",
                "generic-number" => "compare<Box>(new Box());",
                "generic-class" => "compare<Box>(new Box(), new Box());",
                _ => "",
            };
            let check = |operator| {
                check_one(&format!(
                    "{declarations}function compare{parameters}: boolean {{ return {left} {operator} {right}; }}\n\
                     export function main(): void {{ {invocation} }}"
                ))
            };
            let strict_result = check(strict);
            let loose_result = check(loose);
            assert_eq!(
                strict_result.is_ok(),
                expected_code.is_none(),
                "{name} {strict}: {strict_result:?}"
            );
            assert_eq!(
                loose_result.is_ok(),
                strict_result.is_ok(),
                "{name} {loose}: {loose_result:?}"
            );
            if let (Err(strict_errors), Err(loose_errors)) = (strict_result, loose_result) {
                assert_eq!(strict_errors.len(), 1, "{name}: {strict_errors:?}");
                assert_eq!(loose_errors.len(), strict_errors.len(), "{name}");
                assert_eq!(Some(strict_errors[0].code), expected_code, "{name}");
                assert_eq!(loose_errors[0].code, strict_errors[0].code, "{name}");
                assert!(
                    strict_errors[0].message.contains(&format!("`{strict}`")),
                    "{name}"
                );
                assert_eq!(
                    loose_errors[0].message,
                    strict_errors[0].message.replace(strict, loose),
                    "{name}"
                );
            }
        }
    }
}

#[test]
fn loose_equality_matches_strict_absence_operand_rules() {
    for (strict, loose) in [("===", "=="), ("!==", "!=")] {
        let source = format!(
            "type Label = \"a\" | \"b\";\n\
             @Descriptor class Options {{ label?: Label; }}\n\
             export function main(): void {{\n\
               const options: Options = {{}};\n\
               if (options.label {strict} undefined) {{}}\n\
             }}"
        );
        check_one(&source).expect("strict presence test checks");
        check_one(&source.replace(strict, loose)).expect("loose presence test checks");

        let source = format!(
            "export function main(): void {{ const value: i32 = 1; const result = value {strict} undefined; }}"
        );
        let strict_errors = check_one(&source).expect_err("ordinary undefined use fails");
        let loose_errors = check_one(&source.replace(strict, loose))
            .expect_err("ordinary undefined use also fails with loose equality");
        assert_eq!(strict_errors.len(), 1);
        assert_eq!(strict_errors[0].code, RuleCode::S012);
        assert_eq!(loose_errors.len(), strict_errors.len());
        assert_eq!(loose_errors[0].code, strict_errors[0].code);
        assert_eq!(loose_errors[0].message, strict_errors[0].message);
    }
}
