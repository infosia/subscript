use subscript_compiler::{check_program, divergence::Divergence, Pos, RuleCode, SourceFile};

#[test]
fn string_literal_ship_tier_limit() {
    let boundary = format!(
        "export function main(): void {{\n    print(\"{}\");\n}}",
        "a".repeat(65_000)
    );
    assert!(check_program(&[SourceFile::new("boundary.ts", boundary)]).is_ok());

    let oversized = format!(
        "export function main(): void {{\n    print(\"{}\");\n}}",
        "a".repeat(65_001)
    );
    let diagnostics = check_program(&[SourceFile::new("oversized.ts", oversized)]).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, RuleCode::S019);
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::StringLiteralLength)
    );
    assert_eq!(
        diagnostics[0].message,
        "string literal of 65001 bytes exceeds the ship-tier limit of 65,000 bytes"
    );
    assert_eq!(diagnostics[0].pos, Pos::new("oversized.ts", 2, 11));

    let template = format!(
        "export function main(): void {{\n    print(`{}${{true}}{}`);\n}}",
        "",
        "a".repeat(65_001)
    );
    let diagnostics = check_program(&[SourceFile::new("template.ts", template)]).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, RuleCode::S019);
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::StringLiteralLength)
    );
    assert_eq!(
        diagnostics[0].message,
        "string literal of 65001 bytes exceeds the ship-tier limit of 65,000 bytes"
    );
    assert_eq!(diagnostics[0].pos, Pos::new("template.ts", 2, 11));

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
