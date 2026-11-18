//! Literal context checks (compiler.md §336 rules 1 to 3).
//! Cost: short checker calls; corpus sweeps cover both execution tiers.

use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn set_type_arguments_and_plain_templates_check() {
    for form in [
        "new Set<i32>([])",
        "new Set<string>([])",
        "new Set<i32>(([]))",
        "new Set<i32>(true ? [] : [1])",
        "Array.from<i32>([])",
        "\"x\".normalize(`NFC`)",
        "\"x\".normalize(`N\\u0046C`)",
        "\"x\".normalize(\"NFC\")",
        "\"x\".normalize()",
    ] {
        let source = format!("export function main(): void {{ const value = {form}; }}");
        let result = check_program(&[SourceFile::new("literal.ts", source)]);
        assert!(result.is_ok(), "{form}: {result:?}");
    }
    check_program(&[SourceFile::new(
        "literal.ts",
        "export function main(): void { const value: Set<i32> = new Set([]); }",
    )])
    .expect("the annotation gives the empty literal its context");
}

#[test]
fn forms_without_the_required_literal_context_keep_their_diagnostics() {
    for (form, code, reason) in [
        (
            "new Set([])",
            RuleCode::S100,
            "empty array literal without context",
        ),
        ("new Map<i32, i32>([])", RuleCode::S014, "no tuple type"),
        ("\"x\".normalize(`NFD`)", RuleCode::S014, "literal \"NFC\""),
        ("\"x\".normalize(`NFKC`)", RuleCode::S014, "literal \"NFC\""),
        (
            "\"x\".normalize(`NF${\"C\"}`)",
            RuleCode::S014,
            "literal \"NFC\"",
        ),
        ("new Set<i32>([\"wrong\"])", RuleCode::S100, "array element"),
    ] {
        let source = format!("export function main(): void {{ const value = {form}; }}");
        let errors = check_program(&[SourceFile::new("literal.ts", source)])
            .expect_err("the source builds the violating form");
        assert!(
            errors
                .iter()
                .any(|error| error.code == code && error.to_string().contains(reason)),
            "{form}: {errors:?}"
        );
    }
}

#[test]
fn acceptance_entry_checks() {
    check_program(&[SourceFile::new(
        "literal.ts",
        include_str!("../../corpus/accept/a486-literal-context.ts"),
    )])
    .expect("compiler.md §336 acceptance 1");
}
