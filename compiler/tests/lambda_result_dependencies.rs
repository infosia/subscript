//! Section 306: result dependencies through local lambdas and inferred lists.
//! Checker-only regression probes for the amended rules.
use subscript_compiler::{check_program, RuleCode, SourceFile};

fn check(
    source: &str,
) -> Result<subscript_compiler::hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::entry(
        "test.ts",
        format!("{source}\nexport function main(): void {{}}"),
    )])
}

/// Measured cost: 0.01 s; three short checker programs.
#[test]
fn a_local_lambda_carries_result_dependencies() {
    let errors = check("const a = () => { const inner = () => b(); return inner(); };\nconst b = () => { return a(); };\nconst x = a(); const y = b();").expect_err("result cycle");
    assert_eq!(errors.len(), 3, "{errors:?}");
    let mut positions = Vec::new();
    for error in errors {
        assert_eq!(error.code, RuleCode::S100);
        assert_eq!(
            error.message,
            "initializer type depends on its own undecided type"
        );
        positions.push((error.pos.line, error.pos.col));
    }
    positions.sort();
    assert_eq!(positions, [(1, 11), (1, 33), (2, 11)]);
    check("function after(f: () => void): void {} const a = () => { const inner = () => after(b); return inner(); }; const b = () => { return a(); };").expect("local callback value control");
    check("function v(): void {} const a = () => { const inner = () => b(); return v(); }; const b = () => { return a(); };").expect("uncalled local result control");
}

/// Measured cost: 0.01 s; four short checker programs.
#[test]
fn a_list_element_carries_result_dependencies_only_when_called() {
    let errors = check("const a = () => { return table[0](); }; const table = [a];")
        .expect_err("list result cycle");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "initializer type depends on its own undecided type"
    );
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, 11));
    check("function after(fs: (() => void)[]): void {} const a = () => { return after(table); }; const table = [a];").expect("uncalled list element control");
    check("function after(f: () => void): void {} const ping = () => { return after(pong); }; const pong = () => { return after(ping); };").expect("callback control");
    check("const a = () => { return a(); };").expect("direct self-call control");
}

/// Measured cost: 0.01 s; seven short checker programs.
#[test]
fn undefined_is_not_a_type_parameter_name() {
    for source in [
        "function f<undefined>(x: i32): void { return undefined; }",
        "class C<undefined> {}",
        "class C { f<undefined>(): void {} }",
        "type A<undefined> = i32;",
        "const f = <undefined>(x: i32): void => { return undefined; };",
        "type F = <undefined>(x: i32) => void;",
    ] {
        let errors = check(source).expect_err(source);
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].message,
            "parse error: Type parameter name cannot be 'undefined'"
        );
        assert!(errors[0].divergence.is_none());
    }
    check("function f<T>(x: i32): void { return undefined; }").expect("ordinary parameter control");
}

fn mixed_sources(operand: &str) {
    check(&format!("const c = true; const a2 = () => {{ return; }}; const a = () => {{ {operand} }}; const b = () => {{ return a(); }};"))
        .expect("a source outside the cycle decides the value's result");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn mixed_conditional_call_is_accepted() {
    mixed_sources("return (c ? a2 : b)();");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn mixed_conditional_alias_call_is_accepted() {
    mixed_sources("const h = c ? a2 : b; return h();");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn mixed_list_call_is_accepted() {
    mixed_sources("const t = [a2, b]; return t[c ? 0 : 1]();");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn reversed_mixed_list_call_is_accepted() {
    mixed_sources("const t = [b, a2]; return t[c ? 0 : 1]();");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn mixed_module_list_call_is_accepted() {
    check("const a2 = () => { return; }; const a = () => { return table[1](); }; const b = () => { return a(); }; const table = [a2, b];")
        .expect("module list has a source outside the cycle");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn mixed_self_list_call_is_accepted() {
    check("const a2 = () => { return; }; const a = () => { return table[0](); }; const table = [a, a2];")
        .expect("self list has a source outside the cycle");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn list_element_alias_call_is_accepted() {
    // tsc 5.9.2 --strict accepts this initializer before the result read.
    check("const a = () => { const f = table[0]; return f(); }; const table = [a];")
        .expect("element initializer decides the alias signature");
}

/// Measured cost: below 0.01 s; one checker program.
#[test]
fn state_table_with_idle_branch_is_accepted() {
    check_program(&[SourceFile::entry("test.ts", "let ticks: i32 = 0; const idle = () => { print(\"idle\"); }; const walk = () => { ticks = ticks + 1; if (ticks < 3) { return tick(1); } return idle(); }; const states = [idle, walk]; const tick = (s: i32) => { return states[s](); }; export function main(): void { tick(1); }")])
        .expect("idle decides the state table result");
}

/// Measured cost: below 0.01 s; six checker programs.
#[test]
fn values_with_only_cycle_sources_are_rejected() {
    for source in [
        "const a = () => { return table[0](); }; const table = [a];",
        "const a = () => { const t = [b, b]; return t[0](); }; const b = () => { return a(); };",
        "const a = () => { const base = [b]; const t = [...base]; return t[0](); }; const b = () => { return a(); };",
        "const a = () => { return table[0](); }; const table = [[a]][0];",
        "const a = () => { const h = b; return h(); }; const b = () => { return a(); };",
        "const c = true; const a = () => { return (c ? b : b)(); }; const b = () => { return a(); };",
    ] {
        let errors = check(source).expect_err(source);
        assert!(errors.iter().all(|error| error.code == RuleCode::S100 && error.message == "initializer type depends on its own undecided type"), "{source}: {errors:?}");
    }
}

/// Measured cost: below 0.01 s; thirteen checker programs.
#[test]
fn reserved_type_parameter_names_are_rejected() {
    let mut failures = Vec::new();
    for name in [
        "undefined",
        "never",
        "string",
        "object",
        "number",
        "boolean",
        "any",
        "unknown",
        "bigint",
        "symbol",
    ] {
        let source = format!("function f<{name}>(x: i32): void {{ return undefined; }}");
        match check(&source) {
            Err(errors)
                if errors.len() == 1
                    && errors[0].code == RuleCode::S100
                    && errors[0].message
                        == format!("parse error: Type parameter name cannot be '{name}'")
                    && errors[0].divergence.is_none() => {}
            Ok(_) => failures.push(format!("{name}: unexpectedly accepted")),
            Err(errors) => failures.push(format!("{name}: {errors:?}")),
        }
    }
    for name in ["Undefined", "NaN", "i32"] {
        check(&format!("function f<{name}>(): void {{}}"))
            .expect("nonreserved type parameter control");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
