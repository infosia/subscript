use subscript_compiler::{check_program, hir, RuleCode, SourceFile};

#[test]
fn to_fixed_defaults_to_zero_on_both_float_types() {
    for ty in ["f32", "f64"] {
        let source = format!(
            "export function main(): void {{ const x: {ty} = 1.5; const text = x.toFixed(); }}"
        );
        let module = check_program(&[SourceFile::new("test.ts", source)]).unwrap();
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let call = main
            .body
            .iter()
            .find_map(|stmt| match stmt {
                hir::Stmt::Let { init, .. } => match &init.kind {
                    hir::ExprKind::Call {
                        callee: hir::Callee::Num(hir::NumFn::ToFixed),
                        args,
                    } => Some(args),
                    _ => None,
                },
                _ => None,
            })
            .expect("toFixed call");
        assert_eq!(call.len(), 2);
        assert_eq!(call[0].ty, subscript_compiler::types::Type::F64);
        assert!(matches!(call[1].kind, hir::ExprKind::Int(0)));
    }
}

#[test]
fn date_implicit_conversion_keeps_its_diagnostics() {
    for (expression, code, message) in [
        (
            "+d",
            RuleCode::S100,
            "unary operator outside the decided surface",
        ),
        (
            "d - d",
            RuleCode::S100,
            "operator not defined for `Date` and `Date`",
        ),
        (
            "d < d",
            RuleCode::S014,
            "Date values do not compare implicitly",
        ),
        (
            "new Date(d) === d",
            RuleCode::S014,
            "Date values do not compare implicitly",
        ),
    ] {
        let source =
            format!("export function main(): void {{ const d = new Date(0); {expression}; }}");
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1, "{expression}: {errors:?}");
        assert_eq!(errors[0].code, code);
        assert!(errors[0].message.contains(message), "{errors:?}");
    }
    check_program(&[SourceFile::new("test.ts", "export function main(): void { const d = new Date(0); const ms: i64 = new Date(d).valueOf(); print(`${ms - d.getTime()} ${ms < d.valueOf()}`); }")]).expect("explicit conversion control");
}

#[test]
fn date_additions_keep_argument_and_method_value_checks() {
    for expression in [
        "new Date(true)",
        "new Date(...[0])",
        "Date.UTC()",
        "d.valueOf(1)",
        "d.toJSON(1)",
        "d.toUTCString(1)",
        "d.valueOf",
        "d.toJSON",
        "d.toUTCString",
        "(1.5).toFixed(1, 2)",
    ] {
        let source =
            format!("export function main(): void {{ const d = new Date(0); {expression}; }}");
        assert!(
            check_program(&[SourceFile::new("test.ts", source)]).is_err(),
            "{expression}"
        );
    }
    check_program(&[SourceFile::new("test.ts", "export function main(): void { const d = new Date(Date.UTC(2020)); d.valueOf(); d.toJSON(); d.toUTCString(); (1.5).toFixed(); }")]).expect("valid call control");
}
