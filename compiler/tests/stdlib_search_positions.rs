use subscript_compiler::{check_program, hir, RuleCode, SourceFile};

#[test]
fn array_to_string_has_the_join_element_restriction() {
    for receiver in ["[[1, 2], [3]]", "[new Item()]"] {
        for method in ["join", "toString"] {
            let source = format!("class Item {{}} export function main(): void {{ const text = {receiver}.{method}(); }}");
            let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
                .expect_err("non-interpolatable elements are rejected");
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code, RuleCode::S014);
            assert!(diagnostics[0].message.contains("not interpolatable"));
        }
    }
    for receiver in ["[1, 2]", "[true, false]", "[1.5, 2.5]", "[\"a\", \"b\"]"] {
        for method in ["join", "toString"] {
            let source =
                format!("export function main(): void {{ const text = {receiver}.{method}(); }}");
            check_program(&[SourceFile::new("test.ts", source)]).expect("interpolatable control");
        }
    }
}

#[test]
fn aliases_and_defaults_use_existing_intrinsics() {
    let source = r#"export function main(): void {
        const a = " x ".trimLeft();
        const b = " x ".trimRight();
        const c = [1, 2].toString();
        const d = [1, 2].splice(1);
        const e = [1, 2].lastIndexOf(1);
        const f = [1, 2].indexOf(1);
        const g = [1, 2].includes(1);
        const h = "abc".lastIndexOf("a");
        const i = "abc".split("b");
        const j = "abc".split(/b/);
    }"#;
    let module = check_program(&[SourceFile::new("test.ts", source)]).expect("accepted forms");
    let calls: Vec<_> = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap()
        .body
        .iter()
        .filter_map(|stmt| {
            let hir::Stmt::Let { init, .. } = stmt else {
                return None;
            };
            let hir::ExprKind::Call { callee, args } = &init.kind else {
                return None;
            };
            Some((callee, args))
        })
        .collect();
    use hir::{ArrFn as A, Callee as C, StrFn as S};
    for ((callee, args), (expected, default)) in calls.iter().zip([
        (C::Str(S::TrimStart), None),
        (C::Str(S::TrimEnd), None),
        (C::Arr(A::Join), None),
        (C::Arr(A::Splice), Some(i32::MAX)),
        (C::Arr(A::LastIndexOf), Some(i32::MAX)),
        (C::Arr(A::IndexOf), Some(0)),
        (C::Arr(A::Includes), Some(0)),
        (C::Str(S::LastIndexOf), Some(i32::MAX)),
        (C::Str(S::Split), Some(-1)),
        (C::Regex(hir::RegexFn::Split), Some(-1)),
    ]) {
        assert_eq!(**callee, expected);
        if let Some(default) = default {
            assert_eq!(args.len(), 3);
            assert!(
                matches!(args[2].kind, hir::ExprKind::Int(value) if value == i64::from(default))
            );
        }
    }
    assert_eq!(calls.len(), 10);
    assert!(matches!(&calls[2].1[1].kind, hir::ExprKind::Str(value) if value == ","));
}

#[test]
fn new_forms_keep_argument_types_and_arities() {
    for expression in [
        "[1].toString(\",\")",
        "[1].splice()",
        "[1].indexOf(1, 0, 0)",
        "[1].includes(1, \"x\")",
        "[1].lastIndexOf(1, \"x\")",
        "\"abc\".lastIndexOf(\"a\", \"x\")",
        "\"abc\".split(\"b\", \"x\")",
        "\"abc\".split(/b/, \"x\")",
        "\"abc\".trimLeft(1)",
        "\"abc\".trimRight(1)",
    ] {
        let source = format!("export function main(): void {{ const value = {expression}; }}");
        assert!(
            check_program(&[SourceFile::new("test.ts", source)]).is_err(),
            "{expression}"
        );
    }
}
