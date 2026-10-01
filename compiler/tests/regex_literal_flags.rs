use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn unicode_sets_literals_report_one_s100_at_the_literal() {
    for flags in ["v", "gv"] {
        let source =
            format!("export function main(): void {{\n  const regex: RegExp = /a/{flags};\n}}");
        let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
            .expect_err("the v literal flag requires ES2024");
        assert_eq!(diagnostics.len(), 1, "{flags}: {diagnostics:?}");
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code, RuleCode::S100);
        assert_eq!(diagnostic.pos.file, "test.ts");
        assert_eq!((diagnostic.pos.line, diagnostic.pos.col), (2, 25));
        assert_eq!(
            diagnostic.message,
            "the `v` flag requires ES2024 in a regex literal; use `new RegExp(pattern, \"v\")`"
        );
        assert!(diagnostic.divergence.is_none());
    }
}

#[test]
fn unicode_literal_and_unicode_sets_constructor_are_accepted() {
    for expression in ["/a/u", "new RegExp(\"a\", \"v\")"] {
        let source =
            format!("export function main(): void {{ const regex: RegExp = {expression}; }}");
        check_program(&[SourceFile::new("test.ts", source)]).expect("accepted regex control");
    }
}
