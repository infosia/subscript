//! Composite assignability through constraints (§143 rules 1b and 2a).

use subscript_compiler::{check_program, RuleCode, SourceFile};

const PRELUDE: &str = "class Box { v: i32 = 1; } class Other { w: i32 = 2; } class G<A> { value: A; constructor(value: A) { this.value = value; } } class H<A> { value: A; constructor(value: A) { this.value = value; } }";

#[test]
fn each_composite_follows_linked_constraints_and_preserves_identity() {
    for constructor in [
        "@ | null",
        "@[]",
        "FixedArray<@, 2>",
        "Map<string, @>",
        "Map<@, i32>",
        "Set<@>",
        "G<@>",
        "() => @",
        "Generator<@>",
        "Promise<@>",
        "Worker<@, Box>",
        "Inbox<@>",
        "Outbox<@>",
    ] {
        let from = constructor.replace('@', "U");
        let to = constructor.replace('@', "T");
        let source = format!(
            "{PRELUDE} function g<T extends Box, U extends T>(x: {from}): {to} {{ return x; }}"
        );
        assert!(
            check_program(&[SourceFile::new("linked.ts", source)]).is_ok(),
            "{constructor}"
        );
        let source = format!(
            "{PRELUDE} function g<T extends Box, U extends Box>(x: {from}): {to} {{ return x; }}"
        );
        assert!(
            check_program(&[SourceFile::new("unrelated.ts", source)]).is_err(),
            "{constructor}"
        );
    }
}

#[test]
fn function_parameters_are_contravariant_and_returns_are_covariant() {
    for (body, accepts) in [
        (
            "function g<T extends Box, U extends T>(x: (v: T) => U): (v: U) => T { return x; }",
            true,
        ),
        (
            "function g<T extends Box, U extends T>(x: (v: U) => T): (v: T) => U { return x; }",
            false,
        ),
    ] {
        assert_eq!(
            check_program(&[SourceFile::new("variance.ts", format!("{PRELUDE} {body}"))]).is_ok(),
            accepts
        );
    }
}

#[test]
fn concrete_component_rules_stay_with_the_instance() {
    let body = "function g<T extends Box | null>(x: T[]): (Box | null)[] { return x; }";
    assert!(check_program(&[SourceFile::new("generic.ts", format!("{PRELUDE} {body}"))]).is_ok());
    for (body, code, message) in [
        (
            format!("{body} export function main(): void {{ g<Box>([new Box()]); }}"),
            RuleCode::S100,
            "type mismatch: the return value expects `(Box | null)[]`, got `Box[]`",
        ),
        (
            "function g(x: Box[]): (Box | null)[] { return x; }".into(),
            RuleCode::S100,
            "type mismatch: the return value expects `(Box | null)[]`, got `Box[]`",
        ),
        (
            "function g(x: (v: Box) => i32): (v: Box | null) => i32 { return x; }".into(),
            RuleCode::S100,
            "type mismatch: the return value expects `(Box | null) => i32`, got `(Box) => i32`",
        ),
        (
            "function g<T extends Box>(x: FixedArray<T, 2>): FixedArray<Box, 3> { return x; }"
                .into(),
            RuleCode::S100,
            "type mismatch: the return value expects `FixedArray<Box, 3>`, got `FixedArray<T, 2>`",
        ),
        (
            "function g<T extends Box>(x: G<T>): H<Box> { return x; }".into(),
            RuleCode::S005,
            "nominal types are not interchangeable: the return value expects `H<Box>`, got `G<T>`",
        ),
    ] {
        let errors = check_program(&[SourceFile::new("concrete.ts", format!("{PRELUDE} {body}"))])
            .unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, code, "{body}");
        assert_eq!(errors[0].message, message, "{body}");
    }
}

#[test]
fn equality_diagnostics_name_declared_parameters() {
    let source =
        "function g<T extends i32, U extends i32>(x: T, y: U): boolean { return x === y; }";
    let errors = check_program(&[SourceFile::new("names.ts", source)]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, "operator not defined for `T` and `U`");
}

#[test]
fn a_constraint_composite_cannot_supply_a_parameter_composite() {
    for (from, to, code, prefix) in [
        ("Box[]", "T[]", RuleCode::S100, "type mismatch"),
        (
            "G<Box>",
            "G<T>",
            RuleCode::S005,
            "nominal types are not interchangeable",
        ),
        (
            "Map<string, Box[]>",
            "Map<string, T[]>",
            RuleCode::S005,
            "nominal types are not interchangeable",
        ),
        ("() => Box", "() => T", RuleCode::S100, "type mismatch"),
    ] {
        let source =
            format!("{PRELUDE} function g<T extends Box>(x: {from}): {to} {{ return x; }}");
        let errors = check_program(&[SourceFile::new("reverse.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, code);
        assert_eq!(
            errors[0].message,
            format!("{prefix}: the return value expects `{to}`, got `{from}`")
        );
    }
}
