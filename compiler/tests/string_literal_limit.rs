//! §99: decoded byte length does not restrict language acceptance.
use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn string_literals_and_static_template_parts_accept_long_data() {
    for bytes in [64_999, 65_000, 65_001, 65_535, 65_536, 1_048_576] {
        for expression in [
            format!("\"{}\"", "a".repeat(bytes)),
            format!("`{}${{true}}`", "a".repeat(bytes)),
        ] {
            let source = format!("export function main(): void {{ print({expression}); }}");
            assert!(check_program(&[SourceFile::new("long.ts", source)]).is_ok());
        }
    }
    let separate_parts = format!(
        "export function main(): void {{\n    print(`{}${{true}}{}`);\n}}",
        "a".repeat(40_000),
        "b".repeat(40_000)
    );
    assert!(check_program(&[SourceFile::new("separate_parts.ts", separate_parts)]).is_ok());

    let dynamic = format!("export function main(): void {{\n    const text: string = \"{}\";\n    print(`x${{text}}${{text}}y`);\n}}", "a".repeat(40_000));
    assert!(check_program(&[SourceFile::new("dynamic.ts", dynamic)]).is_ok());

    let escaped_text = r"\u00e9".repeat(20_000);
    assert_eq!(escaped_text.len(), 120_000);
    let decoded = format!(
        "export function main(): void {{\n    print(\"{}\");\n}}",
        escaped_text
    );
    assert!(check_program(&[SourceFile::new("decoded.ts", decoded)]).is_ok());
}

#[test]
fn independent_literal_and_template_diagnostics_remain() {
    for (expression, code) in [
        ("256", RuleCode::S008),
        ("`value=${[1, 2]}`", RuleCode::S100),
    ] {
        let source = if expression == "256" {
            format!("export function main(): void {{ const x: u8 = {expression}; }}")
        } else {
            format!("export function main(): void {{ print({expression}); }}")
        };
        let diagnostics = check_program(&[SourceFile::new("diagnostic.ts", source)]).unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == code));
    }
}
