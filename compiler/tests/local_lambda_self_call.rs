//! Named lambda defaults and self-calls (compiler.md §288).
//! Cost: small checker programs; no code generation or external processes.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

fn check(source: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("defaults.ts", source)])
}

#[test]
fn an_omitted_captured_default_reports_once_at_the_default() {
    for (body, calls) in [
        ("return n;", "rec(); rec();"),
        ("if (n > 0) return rec(); return n;", "rec(1);"),
    ] {
        let source = format!(
            "export function main(): void {{\n const captured: i32 = 5;\n const rec = (n: i32 = captured): i32 => {{ {body} }};\n {calls}\n}}"
        );
        let errors = check(&source).expect_err("omitted captured default");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].message,
            "a named lambda default cannot read captured local `captured`"
        );
        assert_eq!(errors[0].pos.line, 3);
        assert_eq!(errors[0].pos.col, 24);
    }
}

#[test]
fn a_passed_captured_default_and_earlier_parameter_controls_check() {
    for declaration in [
        "const rec = (n: i32 = captured): i32 => n; rec(1);",
        "const rec = (n: i32 = captured): i32 => { if (n > 0) return rec(n - 1); return n; }; rec(1);",
        "const rec = (captured: i32, n: i32 = captured): i32 => n; rec(1);",
        "const rec = (n: i32, a: i32 = n + 1, b: i32 = a + 1): i32 => b; rec(1); rec(1, 5);",
    ] {
        check(&format!("export function main(): void {{ const captured: i32 = 5; {declaration} }}"))
            .unwrap_or_else(|errors| panic!("{declaration}: {errors:?}"));
    }
}

#[test]
fn other_function_value_calls_still_require_every_argument() {
    for (declarations, call, name) in [
        ("function ap(h: (a: i32, b: i32) => i32): i32 { return h(1); }", "", "h"),
        ("", "const h: (a: i32, b: i32) => i32 = (a: i32, b: i32 = 5): i32 => a + b; h(1);", "h"),
        ("", "let h = (a: i32, b: i32 = 5): i32 => a + b; h(1);", "h"),
        ("", "const original = (a: i32, b: i32 = 5): i32 => a + b; const h = original; h(1);", "h"),
        ("class C { h: (a: i32, b: i32) => i32 = (a: i32, b: i32 = 5): i32 => a + b; }", "const c = new C(); c.h(1);", "the function value"),
        ("", "const hs: ((a: i32, b: i32) => i32)[] = [(a: i32, b: i32 = 5): i32 => a + b]; hs[0](1);", "the function value"),
        ("", "const h: (a: i32, b: i32) => i32 = (a: i32, b: i32 = 5): i32 => { if (a > 0) return h(a - 1); return b; };", "h"),
    ] {
        let source = format!("{declarations} export function main(): void {{ {call} }}");
        let errors = check(&source).expect_err("function-value arity");
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].message, format!("`{name}` expects 2 argument(s), got 1: a call through a function value passes every parameter"));
    }
}

#[test]
fn omitted_calls_carry_checked_defaults_and_full_calls_keep_their_form() {
    let module = check("export function main(): void { const h = (a: i32, b: i32 = a + 1): i32 => a + b; h(1); h(1, 4); }")
        .expect("named calls check");
    let [hir::Stmt::Let { init, .. }, hir::Stmt::Expr(omitted), hir::Stmt::Expr(passed)] =
        module.functions[0].body.as_slice()
    else {
        panic!("expected lambda and two calls");
    };
    let hir::ExprKind::Lambda { id, .. } = &init.kind else {
        panic!("lambda")
    };
    let hir::ExprKind::Call {
        callee: hir::Callee::NamedLambda { defaults, .. },
        args,
    } = &omitted.kind
    else {
        panic!("named call")
    };
    assert_eq!(defaults.id, *id);
    assert_eq!(defaults.params.len(), 2);
    assert_eq!(defaults.params[1].name, "b");
    assert!(defaults.params[1].default.is_some());
    assert_eq!(args.len(), 1);
    assert!(
        matches!(&passed.kind, hir::ExprKind::Call { callee: hir::Callee::Value(_), args } if args.len() == 2)
    );
}

#[test]
fn module_calls_before_the_declaration_and_global_defaults_check() {
    check("function use(): i32 { return h(2); } const h = (a: i32, b: i32 = a + 1): i32 => a + b; export function main(): void { use(); }")
        .expect("module declaration order");
    check("const global: i32 = 5; const h = (n: i32 = global): i32 => n; export function main(): void { h(); }")
        .expect("a module global is not a captured local");
}

#[test]
fn a_global_lambda_default_reports_each_error_once() {
    for (default, code) in [("5", RuleCode::S100), ("missing", RuleCode::S016)] {
        for source in [
            format!("const top = (k: i32, s: string = {default}): string => s; top(1); export function main(): void {{}}"),
            format!("function before(): string {{ return top(1); }} const top = (k: i32, s: string = {default}): string => s; export function main(): void {{}}"),
        ] {
            let errors = check(&source).expect_err("invalid default");
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
            assert_eq!(errors[0].code, code, "{errors:?}");
        }
    }
}

#[test]
fn a_capture_error_in_a_global_lambda_default_reports_once() {
    for source in [
        "const top = (k: i32, f: () => i32 = (): i32 => k): i32 => f(); top(1); export function main(): void {}",
        "function before(): i32 { return top(1); } const top = (k: i32, f: () => i32 = (): i32 => k): i32 => f(); export function main(): void {}",
    ] {
        let errors = check(source).expect_err("a default lambda cannot capture a mutable parameter");
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S009, "{errors:?}");
    }
}
