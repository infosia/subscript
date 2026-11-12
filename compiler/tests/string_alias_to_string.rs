//! String places convert alias values (compiler.md section 259).
use subscript_compiler::{check_program, hir, RuleCode, SourceFile, Type};

#[test]
fn string_places_carry_an_explicit_conversion() {
    for alias in ["\"a\" | \"b\"", "CEnum<{ a: -3; b: 7; }>"] {
        for form in [
            "const s: string = m;",
            "let s: string = \"\"; s = m;",
            "show(m); print(m);",
            "new P(m);",
            "const xs: string[] = []; xs.push(m);",
            "return m;",
            "const n: Mode = m; const f = (): string => n;",
            "const xs: string[] = [m, \"c\"];",
            "const p = new P(\"\"); p.s = m;",
            "const s: string | null = m;",
        ] {
            let source = format!("type Mode = {alias}; class P {{ constructor(public s: string) {{}} }} function show(s: string): void {{}} function f(m: Mode): string {{ {form} return \"\"; }}");
            let module = check_program(&[SourceFile::new("test.ts", source)])
                .unwrap_or_else(|e| panic!("{form}: {e:?}"));
            let mut conversions = 0;
            for function in &module.functions {
                for statement in &function.body {
                    conversions += count_conversions(hir::HirChild::Stmt(statement));
                }
            }
            assert_eq!(
                conversions,
                if form == "show(m); print(m);" { 2 } else { 1 },
                "{form}"
            );
        }
        let source =
            format!("type Mode = {alias}; function f(m: Mode): string | undefined {{ return m; }}");
        let module = check_program(&[SourceFile::new("test.ts", source)]).unwrap();
        let hir::Stmt::Return {
            value: Some(value), ..
        } = &module.functions[0].body[0]
        else {
            panic!("return");
        };
        assert_eq!(value.ty, Type::Str);
        assert_eq!(count_conversions(hir::HirChild::Expr(value)), 1);
        assert!(matches!(
            value.trap_sites(&module).as_slice(),
            [hir::TrapSite::Allocation { .. }]
        ));
        assert!(!module.expression_can_raise(value));
    }
}

#[test]
fn other_alias_operations_stay_rejected() {
    for (form, message) in [
        ("const x: Mode = s;", "expects `Mode`, got `string`"),
        ("const x: Other = m;", "expects `Other`, got `Mode`"),
        (
            "switch (s) { case m: break; }",
            "type mismatch: the case label expects `string`, got `Mode`",
        ),
        ("m === s;", "operator `===`"),
        ("m !== s;", "operator `!==`"),
        ("m.length;", "has no member `length`"),
        ("\"x\" + m;", "operator not defined"),
        (
            "const xs: string[] = ms;",
            "expects `string[]`, got `Mode[]`",
        ),
        ("const f: () => string = g;", "expects `() => string`"),
    ] {
        let source = format!("type Mode = \"a\" | \"b\"; type Other = \"a\" | \"b\"; function f(m: Mode, s: string, ms: Mode[], g: () => Mode): void {{ {form} }}");
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == RuleCode::S100 && e.message.contains(message)),
            "{form}: {errors:?}"
        );
    }
}

fn count_conversions(child: hir::HirChild<'_>) -> usize {
    let (own, children) = match child {
        hir::HirChild::Expr(value) => {
            let own = if let hir::ExprKind::Cast(operand) = &value.kind {
                if matches!(operand.ty, Type::StringAlias(_)) {
                    assert_eq!(
                        operand.ty,
                        Type::StringAlias(subscript_compiler::StringAliasId(0))
                    );
                    assert_eq!(value.ty, Type::Str);
                    1
                } else {
                    0
                }
            } else {
                0
            };
            (own, value.children())
        }
        hir::HirChild::Stmt(statement) => (0, statement.children()),
    };
    own + children.into_iter().map(count_conversions).sum::<usize>()
}
