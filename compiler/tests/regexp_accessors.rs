use subscript_compiler::{check_program, hir, types::Type, SourceFile};

#[test]
fn checker_accepts_sticky_boolean_for_every_accepted_flag_set() {
    for mask in 0..128 {
        let flags: String = b"dgimsuv"
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, flag)| char::from(*flag))
            .collect();
        if flags.contains('u') && flags.contains('v') {
            continue;
        }
        let source =
            format!("export function main(): void {{ const value: boolean = /a/{flags}.sticky; }}");
        let module =
            check_program(&[SourceFile::new("test.ts", source)]).expect("accepted flag set");
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let init = main
            .body
            .iter()
            .find_map(|stmt| match stmt {
                hir::Stmt::Let { init, .. }
                    if matches!(
                        init.kind,
                        hir::ExprKind::Call {
                            callee: hir::Callee::Regex(hir::RegexFn::Sticky),
                            ..
                        }
                    ) =>
                {
                    Some(init)
                }
                _ => None,
            })
            .expect("sticky reads the runtime flag set");
        assert_eq!(init.ty, Type::Bool);
    }
    let diagnostics = check_program(&[SourceFile::new(
        "test.ts",
        "export function main(): void { const value = /a/y.sticky; }",
    )])
    .expect_err("sticky flag is rejected");
    assert!(diagnostics.iter().any(|d| d.message.contains("lastIndex")));
}

#[test]
fn regexp_accessors_are_read_only_and_to_string_takes_no_arguments() {
    for expression in [
        "/a/.global = true",
        "/a/.sticky = false",
        "/a/.toString(1)",
        "/a/.toString",
        "/a/.unicodeSets",
    ] {
        let source = format!("export function main(): void {{ {expression}; }}");
        assert!(
            check_program(&[SourceFile::new("test.ts", source)]).is_err(),
            "{expression}"
        );
    }
    check_program(&[SourceFile::new(
        "test.ts",
        "export function main(): void { const value = /a/.toString(); }",
    )])
    .expect("valid call control");
}
