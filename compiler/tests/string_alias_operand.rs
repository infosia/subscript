//! String operand conversions and the alias controls of compiler.md §299.
//! Measured cost: 0.06 s in debug for the four tests; checker/HIR only.
use subscript_compiler::{check_program, hir, RuleCode, SourceFile, StringAliasId, Type};

fn module(alias: &str, body: &str) -> hir::Module {
    check_program(&[SourceFile::new("test.ts", format!("type Mode = {alias}; function f(m: Mode, m2: Mode, s: string, n: string | null): void {{ {body} }}"))]).unwrap_or_else(|e| panic!("{body}: {e:?}"))
}

fn conversions(child: hir::HirChild<'_>) -> usize {
    let (own, children) = match child {
        hir::HirChild::Expr(e) => (
            usize::from(
                matches!(&e.kind, hir::ExprKind::Cast(inner) if matches!(inner.ty, Type::StringAlias(_)) && e.ty == Type::Str),
            ),
            e.children(),
        ),
        hir::HirChild::Stmt(s) => (0, s.children()),
    };
    own + children.into_iter().map(conversions).sum::<usize>()
}

#[test]
fn operand_places_record_conversions_for_plain_and_wire_aliases() {
    for alias in ["\"a\" | \"bb\"", "CEnum<{ a: -3; bb: 7; }>"] {
        for (body, count) in [
            ("m === s; s !== m; m !== s; s === m;", 4),
            ("m === n; n !== m;", 2),
            ("const c: string = \"zzz\"; m === c; c !== m;", 2),
            ("const c: string | null = \"zzz\"; m === c; c !== m;", 2),
            ("const c: Mode = \"a\"; m === c; c !== m;", 0),
            ("m.length; m.toUpperCase(); m.includes(\"a\");", 3),
            ("m as string;", 1),
            ("\"x\" + m; m + \"x\"; m + m2; m + \"a\";", 5),
            ("let acc: string = \"\"; acc += m;", 1),
            ("switch (s) { case m: break; }", 1),
            ("m === \"a\"; m !== \"a\"; m === m2; m !== m2;", 0),
        ] {
            let module = module(alias, body);
            let function = module.functions.iter().find(|f| f.name == "f").unwrap();
            assert_eq!(
                function
                    .body
                    .iter()
                    .map(|s| conversions(hir::HirChild::Stmt(s)))
                    .sum::<usize>(),
                count,
                "{body}"
            );
        }
        for (literal, element) in [
            ("[m, \"c\"]", Type::Str),
            ("[\"c\", m]", Type::Str),
            ("[m, \"a\", s]", Type::Str),
            ("[s, m]", Type::Str),
            ("[m, \"a\"]", Type::Str),
            ("[\"a\", m]", Type::Str),
            ("[m, m2, \"a\"]", Type::Str),
            ("[m, m2]", Type::StringAlias(StringAliasId(0))),
        ] {
            let module = module(alias, &format!("const xs = {literal};"));
            let function = module.functions.iter().find(|f| f.name == "f").unwrap();
            let hir::Stmt::Let { ty, .. } = &function.body[0] else {
                panic!("array binding")
            };
            assert_eq!(*ty, Type::array(element), "{literal}");
        }
    }
    let source = "type Mode = \"a\" | \"bb\"; function s(): string | undefined { return \"bb\"; } function f(m: Mode): void { m === s(); s() !== m; }";
    check_program(&[SourceFile::new("undefined.ts", source)]).unwrap();
}

#[test]
fn unrelated_aliases_and_nonmembers_stay_rejected() {
    for (body, message) in [
        (
            "const x: Mode = s;",
            "type mismatch: the initializer expects `Mode`, got `string`",
        ),
        (
            "const x: Other = m;",
            "type mismatch: the initializer expects `Other`, got `Mode`",
        ),
        (
            "m === other;",
            "operator `===` not defined for `Mode` and `Other`",
        ),
        ("m + other;", "operator not defined for `Mode` and `Other`"),
        (
            "m === \"c\";",
            "operator `===` not defined for `Mode` and `string`",
        ),
        (
            "const xs: string[] = ms;",
            "type mismatch: the initializer expects `string[]`, got `Mode[]`",
        ),
        ("m.length = 2;", "`length` is outside the string surface (length, slice, `+` concatenation, `===`/`!==`, and the Q21 String methods)"),
        ("m.unknown();", "`unknown` is outside the string surface (length, slice, `+` concatenation, `===`/`!==`, and the Q21 String methods)"),
        ("m - s;", "operator not defined for `Mode` and `string`"),
        (
            "m == s;",
            "operator `==` not defined for `Mode` and `string`",
        ),
    ] {
        let source = format!("type Mode = \"a\" | \"bb\"; type Other = \"a\" | \"bb\"; function f(m: Mode, other: Other, s: string, ms: Mode[]): void {{ {body} }}");
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].message, message, "{body}");
    }
}

#[test]
fn string_relational_comparisons_keep_the_present_diagnostic() {
    for alias in ["\"a\" | \"bb\"", "CEnum<{ a: -3; bb: 7; }>"] {
        for body in [
            "m < s;", "m <= s;", "m > s;", "m >= s;", "s < m;", "s <= m;", "s > m;", "s >= m;",
        ] {
            let source =
                format!("type Mode = {alias}; function f(m: Mode, s: string): void {{ {body} }}");
            let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
            assert_eq!(errors.len(), 1, "{body}: {errors:?}");
            assert_eq!(errors[0].code, RuleCode::S100);
            let types = if body.starts_with('m') {
                "`Mode` and `string`"
            } else {
                "`string` and `Mode`"
            };
            assert_eq!(
                errors[0].message,
                format!("operator not defined for {types}")
            );
        }
    }
}

// Measured cost: included in the 0.06 s checker suite; no execution tier.
#[test]
fn literal_operands_keep_integer_comparisons_and_nonmember_diagnostics() {
    for alias in ["\"a\" | \"bb\"", "CEnum<{ a: -3; bb: 7; }>"] {
        for literal in [
            "\"a\"",
            "`a`",
            "((\"a\"))",
            "local",
            "chain",
            "moduleConst",
            "moduleChain",
        ] {
            let source = format!("type Mode = {alias}; const moduleConst = \"a\"; const moduleChain = (moduleConst); function f(m: Mode): void {{ const local = (`a`); const chain = (local); m === {literal}; {literal} !== m; switch (m) {{ case {literal}: break; default: break; }} switch ({literal}) {{ case m: break; }} }}");
            let checked = check_program(&[SourceFile::new("literal.ts", source)]).unwrap();
            let f = checked.functions.iter().find(|f| f.name == "f").unwrap();
            assert_eq!(
                f.body
                    .iter()
                    .map(|s| conversions(hir::HirChild::Stmt(s)))
                    .sum::<usize>(),
                0,
                "{literal}"
            );
        }
        let source = format!("type Mode = {alias}; const MS: string = \"zzz\"; function f(m: Mode): void {{ m === MS; MS !== m; }}");
        let checked = check_program(&[SourceFile::new("annotated.ts", source)]).unwrap();
        let f = checked.functions.iter().find(|f| f.name == "f").unwrap();
        assert_eq!(
            f.body
                .iter()
                .map(|s| conversions(hir::HirChild::Stmt(s)))
                .sum::<usize>(),
            2,
        );
        for literal in [
            "\"zzz\"",
            "`zzz`",
            "((\"zzz\"))",
            "local",
            "chain",
            "moduleConst",
            "moduleChain",
        ] {
            for (body, message) in [
                (
                    format!("m === {literal};"),
                    "operator `===` not defined for `Mode` and `string`",
                ),
                (
                    format!("{literal} !== m;"),
                    "operator `!==` not defined for `string` and `Mode`",
                ),
                (
                    format!("switch (m) {{ case {literal}: break; default: break; }}"),
                    "case label \"zzz\" is not a member of string-literal union alias `Mode`",
                ),
                (
                    format!("switch ({literal}) {{ case m: break; }}"),
                    "case label \"zzz\" is not a member of string-literal union alias `Mode`",
                ),
            ] {
                let source = format!("type Mode = {alias}; const moduleConst = \"zzz\"; const moduleChain = (moduleConst); function f(m: Mode): void {{ const local = (`zzz`); const chain = (local); {body} }}");
                let errors = check_program(&[SourceFile::new("literal.ts", source)]).unwrap_err();
                assert_eq!(errors.len(), 1, "{body}: {errors:?}");
                assert_eq!(errors[0].code, RuleCode::S100);
                assert_eq!(errors[0].message, message, "{body}");
            }
        }
    }
}

// Measured cost: 0.01 s for this checker/HIR test; no execution tier.
#[test]
fn readonly_fields_follow_the_literal_operand_table() {
    let fields = "class K { static readonly Z = \"zzz\"; static readonly A = \"aaa\"; readonly w = \"zzz\"; readonly b = \"bbb\"; static readonly S: string = \"zzz\"; }";
    for (body, count) in [
        ("m === K.A;", 0),
        ("m === k.b;", 0),
        ("m === K.S;", 1),
        ("switch (m) { case K.A: break; case k.b: break; }", 0),
    ] {
        let source = format!(
            "type Mode = \"aaa\" | \"bbb\"; {fields} function f(m: Mode, k: K): void {{ {body} }}"
        );
        let checked = check_program(&[SourceFile::new("fields.ts", source)]).unwrap();
        let f = checked.functions.iter().find(|f| f.name == "f").unwrap();
        assert_eq!(
            f.body
                .iter()
                .map(|s| conversions(hir::HirChild::Stmt(s)))
                .sum::<usize>(),
            count,
            "{body}"
        );
    }
    for operand in ["K.Z", "k.w"] {
        let source = format!("type Mode = \"aaa\" | \"bbb\"; {fields} function f(m: Mode, k: K): void {{ m === {operand}; }}");
        let errors = check_program(&[SourceFile::new("fields.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].message,
            "operator `===` not defined for `Mode` and `string`"
        );
    }
    for constructor in [
        "n: i32; constructor(n: i32) { this.n = n; }",
        "constructor() { this.w = \"zzz\"; }",
    ] {
        let source = format!("type Mode = \"aaa\" | \"bbb\"; class K {{ readonly w = \"zzz\"; {constructor} }} function f(m: Mode, k: K): void {{ m === k.w; }}");
        let errors = check_program(&[SourceFile::new("constructor.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].message,
            "operator `===` not defined for `Mode` and `string`"
        );
    }
    for value in ["\"zzz\"", "\"yyy\"", "value"] {
        let source = format!("class K {{ readonly w = \"zzz\"; constructor(value: string) {{ this.w = {value}; }} }}");
        let result = check_program(&[SourceFile::new("store.ts", &source)]);
        if value == "\"zzz\"" {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(
                errors[0].message,
                "readonly field assignment expects literal \"zzz\""
            );
        }
    }
    let errors = check_program(&[SourceFile::new(
        "subclass.ts",
        "class K { readonly w = \"zzz\"; } class D extends K { readonly w = \"zzz\"; }",
    )])
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message == "class inheritance is not in the decided surface"));
}

#[test]
fn readonly_numeric_and_boolean_constructor_stores_preserve_literals() {
    for (initial, same, different, parameter) in [
        ("0", "(0)", "1", "i32"),
        ("false", "false", "true", "boolean"),
        ("true", "true", "false", "boolean"),
        ("-2", "(-2)", "2", "i32"),
        ("0.0", "0", "1", "f64"),
        ("0x10", "16", "17", "i32"),
    ] {
        for value in [same, different, "c"] {
            let source = format!("class K {{ readonly n = {initial}; constructor(c: {parameter}) {{ this.n = {value}; }} }}");
            let result = check_program(&[SourceFile::new("store.ts", &source)]);
            if value == same {
                result.unwrap();
            } else {
                let errors = result.unwrap_err();
                assert_eq!(errors.len(), 1, "{source}: {errors:?}");
                assert_eq!(
                    errors[0].message,
                    format!("readonly field assignment expects literal {initial}")
                );
            }
        }
    }
}

#[test]
fn readonly_initializer_this_reads_keep_their_literal() {
    for literal in ["zzz", "aaa"] {
        let source = format!("type Mode = \"aaa\" | \"bbb\"; class K {{ readonly a = \"{literal}\"; readonly b = this.a; readonly c = this.b; }} function f(m: Mode, k: K): void {{ m === k.c; }}");
        let result = check_program(&[SourceFile::new("fields.ts", source)]);
        if literal == "aaa" {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(
                errors[0].message,
                "operator `===` not defined for `Mode` and `string`"
            );
        }
    }
}
