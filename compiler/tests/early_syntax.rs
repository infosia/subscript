//! Deferred syntax diagnostics preserve checker precedence (compiler.md §341 rule 4c).
use subscript_compiler::{
    check_program, discover_module_sources, Diagnostic, RuleCode, SourceFile,
};

const OCTAL: &str =
    "parse error: '0'-prefixed octal literals and octal escape sequences are deprecated";

fn errors(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::entry("syntax.ts", source)])
        .expect_err("invalid TypeScript must fail")
}

fn syntax_error(source: &str, expected: &str) {
    let errors = errors(source);
    assert!(errors.iter().all(|error| error.code == RuleCode::S100 && error.message.starts_with("parse error: ")), "{errors:?}");
    assert!(
        errors.iter().any(|error| error.message == expected),
        "{errors:?}"
    );
}

// Cost (debug): less than 0.01 s; two checker calls, without subprocesses.
#[test]
fn legacy_octal_has_no_checked_value() {
    for ty in ["i32", "f64"] {
        syntax_error(
            &format!("export function main(): void {{ const m: {ty} = 0755; }}"),
            OCTAL,
        );
    }
}

// Cost (debug): less than 0.01 s; one checker call, without subprocesses.
#[test]
fn leading_zero_decimal_is_a_syntax_error() {
    syntax_error(
        "export function main(): void { const m: i32 = 09; }",
        "parse error: Decimals with leading zeros are not allowed in strict mode",
    );
}

// Cost (debug): less than 0.01 s; two checker calls, without subprocesses.
#[test]
fn duplicate_constructor_implementations_are_syntax_errors() {
    syntax_error(
        "class C { constructor() {} constructor() {} } export function main(): void {}",
        "parse error: Multiple constructor implementations are not allowed.",
    );
    check_program(&[SourceFile::entry(
        "control.ts",
        "class C { constructor() {} } export function main(): void {}",
    )])
    .expect("one constructor control");
}

// Cost (debug): less than 0.01 s; nine checker calls, without subprocesses.
#[test]
fn each_strict_reserved_binding_is_a_syntax_error() {
    for name in [
        "implements",
        "interface",
        "let",
        "package",
        "private",
        "protected",
        "public",
        "static",
        "yield",
    ] {
        syntax_error(
            &format!("export function main(): void {{ const {name}: i32 = 1; }}"),
            &format!("parse error: The keyword '{name}' is reserved"),
        );
    }
}

// Cost (debug): less than 0.01 s; two checker calls, without subprocesses.
#[test]
fn eval_and_arguments_bindings_are_syntax_errors() {
    for (kind, name) in [("const", "eval"), ("let", "arguments")] {
        syntax_error(
            &format!("export function main(): void {{ {kind} {name}: i32 = 1; }}"),
            &format!("parse error: Cannot assign to '{name}' in strict mode"),
        );
    }
}

// Cost (debug): less than 0.01 s; two checker calls, without subprocesses.
#[test]
fn octal_escape_spellings_are_syntax_errors() {
    for escape in [r"\01", r"\101"] {
        syntax_error(
            &format!(r#"export function main(): void {{ const text: string = "{escape}"; }}"#),
            OCTAL,
        );
    }
}

// Cost (debug): less than 0.01 s; two checker calls, without subprocesses.
#[test]
fn strict_directive_checks_the_unrewritten_parameter_list() {
    syntax_error(
        r#"function f(n: i32 = 1): void { "use strict"; } export function main(): void {}"#,
        "parse error: Illegal 'use strict' directive in function with non-simple parameter list",
    );
    check_program(&[SourceFile::entry(
        "control.ts",
        r#"function f(n: i32): void { "use strict"; } export function main(): void {}"#,
    )])
    .expect("simple parameter control");
}

// Cost (debug): less than 0.01 s; one checker call, without subprocesses.
#[test]
fn r151_keeps_its_checker_diagnostic() {
    let errors = errors(include_str!("../../corpus/reject/r151-duplicate-const.ts"));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S017);
    assert_eq!(
        errors[0].message,
        "duplicate declaration of `value` in one scope"
    );
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (9, 9));
}

// Cost (debug): less than 0.01 s; one checker call, without subprocesses.
#[test]
fn an_unrelated_checker_error_wins_across_files_and_discovery() {
    let entry = SourceFile::entry(
        "main.ts",
        "import { n } from './dep'; const bad: any = 1; export function main(): void {}",
    );
    let files = discover_module_sources(
        ("main".to_owned(), entry),
        |_, specifier| {
            Ok::<_, ()>((specifier == "./dep").then(|| {
                (
                    "dep".to_owned(),
                    SourceFile::new("dep.ts", "export const n: i32 = 0755;"),
                )
            }))
        },
        |_, errors| panic!("discovery must defer semantic errors: {errors:?}"),
    )
    .expect("discover the complete program");
    assert_eq!(files.len(), 2);
    let errors = check_program(&files).expect_err("the any annotation fails");
    assert!(!errors.is_empty());
    assert!(
        errors.iter().all(
            |error| error.code == RuleCode::S001 && !error.message.starts_with("parse error: ")
        ),
        "{errors:?}"
    );
}

// Cost (debug): less than 0.01 s; one checker call, without subprocesses.
#[test]
fn deferred_errors_keep_their_file_base_and_utf16_column() {
    let errors = check_program(&[
        SourceFile::ambient("mirror.d.ts", "declare const K = 1;"),
        SourceFile::entry(
            "main.ts",
            "export function main(): void {\n const s = \"👍\"; const x: i32 = 0755;\n}",
        ),
    ])
    .expect_err("the octal initializer has no value");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, OCTAL);
    assert_eq!(errors[0].pos.file, "main.ts");
    assert_eq!(errors[0].pos.line, 2);
    assert_eq!(errors[0].pos.col, 33);
}

// Cost (debug): 0.04 s; one checker call, without subprocesses.
#[test]
fn ambient_function_parameter_names_keep_their_declaration_context() {
    check_program(&[
        SourceFile::ambient(
            "mirror.d.ts",
            "// @subscript-c-header include=\"syntax.h\"\ndeclare function host(arguments: i32): void;",
        ),
        SourceFile::entry("main.ts", "export function main(): void { host(1); }"),
    ])
    .expect("an ambient declaration permits this parameter name");
}
