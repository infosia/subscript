//! Value flow under compiler.md §332 rules 1 to 4.
//! Cost: checker calls only; the standing corpus suite runs both execution tiers.

use subscript_compiler::{check_program, render_diagnostics, RuleCode, SourceFile};

fn pair(declarations: &str, safe_call: &str, violating_call: &str, route: &str) {
    let source = |call: &str| {
        format!(
            "{declarations}\nconst good = (): void => {{ print(\"good\"); }};\n\
         const bad = (): void => {{ print(`${{LATER}}`); }};\n\
         {call}\nconst LATER: i32 = 5;\nexport function main(): void {{}}"
        )
    };
    check_program(&[SourceFile::entry("main.ts", source(safe_call))])
        .expect("the same call shape follows only safe values");
    let files = [SourceFile::entry("main.ts", source(violating_call))];
    let errors = check_program(&files).expect_err("the call reaches the later global");
    assert_eq!(errors.len(), 1, "{}", render_diagnostics(&files, &errors));
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        format!("`LATER` is accessed before its declaration, through {route}")
    );
}

#[test]
fn a_direct_call_passes_its_value_to_the_called_parameter() {
    pair(
        "function run(f: () => void): void { f(); }",
        "run(good);",
        "run(bad);",
        "`run` -> an indirect call",
    );
}

#[test]
fn parameters_pass_values_through_two_helpers_and_const_aliases() {
    pair("function run(f: () => void): void { const alias = f; alias(); }\nfunction outer(g: () => void): void { const alias = g; run(alias); }", "outer(good);", "outer(bad);", "`outer` -> `run` -> an indirect call");
}

#[test]
fn a_known_indirect_call_passes_its_value_to_the_called_parameter() {
    pair(
        "const run = (f: () => void): void => { f(); };",
        "run(good);",
        "run(bad);",
        "an indirect call",
    );
}

#[test]
fn an_immediate_lambda_passes_its_value_to_the_called_parameter() {
    pair(
        "",
        "((f: () => void): void => { f(); })(good);",
        "((f: () => void): void => { f(); })(bad);",
        "an indirect call",
    );
}

#[test]
fn module_and_local_const_aliases_keep_their_sources() {
    pair(
        "function run(f: () => void): void { const alias = f; alias(); }",
        "const a = good; const b = a; run(b);",
        "const a = bad; const b = a; run(b);",
        "`run` -> an indirect call",
    );
    pair(
        "function run(f: () => void): void { const one = f; const two = one; two(); }",
        "run(good);",
        "run(bad);",
        "`run` -> an indirect call",
    );
}

#[test]
fn declared_functions_used_as_const_values_keep_their_units() {
    pair("function safe(): void {}\nfunction late(): void { print(`${LATER}`); }\nfunction run(f: () => void): void { const alias = f; alias(); }", "const f = safe; run(f);", "const f = late; run(f);", "`run` -> an indirect call");
}

#[test]
fn a_conditional_callee_and_its_arguments_use_both_branches() {
    pair(
        "function run(f: () => void, g: () => void, choice: boolean): void { (choice ? f : g)(); }",
        "run(good, good, true);",
        "run(good, bad, true);",
        "`run` -> an indirect call",
    );
    pair("const one = (f: () => void): void => { f(); }; const two = (f: () => void): void => { f(); };", "(true ? one : two)(good);", "(true ? one : two)(bad);", "an indirect call");
}

#[test]
fn nullish_callees_keep_the_parameter_and_the_fallback() {
    pair("function run(f: (() => void) | null): void { const selected = f ?? (() => {}); selected(); }", "run(good);", "run(bad);", "`run` -> an indirect call");
}

#[test]
fn a_builtin_callback_follows_only_the_argument_values() {
    pair(
        "function run(xs: i32[], f: (x: i32) => i32): void { print(`${xs.map(f).join(\",\")}`); }",
        "run([1], (x: i32): i32 => x + 1);",
        "run([1], (x: i32): i32 => x + LATER);",
        "`run` -> an indirect call",
    );
}

#[test]
fn defaults_supply_values_to_their_parameters() {
    pair(
        "function run(f: () => void = (): void => {}): void { f(); }",
        "run();",
        "run(bad);",
        "`run` -> an indirect call",
    );
    let source = |read: &str| {
        format!("function run(f: () => void = (): void => {{ print({read}); }}): void {{ f(); }}\nrun();\nconst LATER: i32 = 5;\nexport function main(): void {{}}")
    };
    check_program(&[SourceFile::entry("main.ts", source("\"safe\""))]).expect("safe default");
    let errors = check_program(&[SourceFile::entry("main.ts", source("`${LATER}`"))])
        .expect_err("the default value runs");
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].message,
        "`LATER` is accessed before its declaration, through `run` -> an indirect call"
    );
}

#[test]
fn an_escape_propagates_back_through_direct_and_known_indirect_arguments() {
    for declaration in [
        "function save(f: () => void): () => void { return f; }",
        "const save = (f: () => void): () => void => f;",
    ] {
        pair(
            declaration,
            "const saved = save(good); let hook: () => void = good; hook();",
            "const saved = save(bad); let hook: () => void = good; hook();",
            "an indirect call",
        );
    }
}

#[test]
fn unknown_call_arguments_and_container_stores_escape() {
    pair(
        "let register: (f: () => void) => void = (f: () => void): void => {};",
        "register(good);",
        "register(bad);",
        "an indirect call",
    );
    pair(
        "",
        "const fs: (() => void)[] = [good]; let hook = good; hook();",
        "const fs: (() => void)[] = [bad]; let hook = good; hook();",
        "an indirect call",
    );
    pair(
        "function pick(f: () => void): () => void { return f; }",
        "pick(good)();",
        "pick(bad)();",
        "an indirect call",
    );
}

#[test]
fn an_escaped_made_unit_receives_escaped_parameter_values() {
    pair(
        "let run: (f: () => void) => void = (f: () => void): void => { f(); };",
        "run(good);",
        "run(bad);",
        "an indirect call",
    );
}

#[test]
fn a_made_but_uncalled_body_does_not_add_nested_values_to_m() {
    pair(
        "const maker = (): () => void => (): void => { print(`${LATER}`); };",
        "let hook = good; hook();",
        "const saved = maker(); let hook = good; hook();",
        "an indirect call",
    );
}

#[test]
fn lexical_shadowing_does_not_replace_an_outer_parameter_source() {
    pair(
        "function run(f: () => void): void { { const f = (): void => {}; f(); } f(); }",
        "run(good);",
        "run(bad);",
        "`run` -> an indirect call",
    );
    pair("function run(f: () => void): void { if (true) { const f = (): void => {}; f(); } else { f(); } }", "run(good);", "run(bad);", "`run` -> an indirect call");
}

#[test]
fn const_binding_calls_and_immediate_calls_follow_their_own_bodies() {
    pair(
        "",
        "const f = (): void => {}; f();",
        "const f = (): void => { print(`${LATER}`); }; f();",
        "an indirect call",
    );
    pair(
        "",
        "((): void => {})();",
        "((): void => { print(`${LATER}`); })();",
        "an indirect call",
    );
}

#[test]
fn each_reject_entry_pins_its_full_route() {
    for source in [
        include_str!("../../corpus/reject/r548-an-indirect-call-runs-its-argument.ts"),
        include_str!("../../corpus/reject/r549-an-indirect-call-runs-a-forwarded-argument.ts"),
        include_str!("../../corpus/reject/r550-an-indirect-call-runs-an-escaped-value.ts"),
        include_str!("../../corpus/reject/r551-an-indirect-call-runs-an-immediate-lambda.ts"),
    ] {
        let expected = source
            .lines()
            .find_map(|line| line.strip_prefix("// expected-error: S100: "))
            .unwrap();
        let errors = check_program(&[SourceFile::entry("main.ts", source)])
            .expect_err("the corpus form violates initialization order");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].message, expected);
    }
}

#[test]
fn methods_and_constructors_pass_values_to_their_own_parameters() {
    pair(
        "class Runner { run(f: () => void): void { f(); } }",
        "const runner = new Runner(); runner.run(good);",
        "const runner = new Runner(); runner.run(bad);",
        "`Runner.run` -> an indirect call",
    );
    pair(
        "class Runner { constructor(f: () => void) { f(); } }",
        "new Runner(good);",
        "new Runner(bad);",
        "`Runner.constructor` -> an indirect call",
    );
}

#[test]
fn a_parameter_escape_propagates_through_a_forwarding_helper() {
    pair("function save(f: () => void): () => void { return f; } function forward(g: () => void): void { save(g); }", "forward(good); let hook = good; hook();", "forward(bad); let hook = good; hook();", "an indirect call");
}

#[test]
fn a_field_call_follows_escaped_values() {
    pair(
        "class Holder { hook: () => void = (): void => {}; }",
        "const holder = new Holder(); holder.hook = good; holder.hook();",
        "const holder = new Holder(); holder.hook = bad; holder.hook();",
        "an indirect call",
    );
}

#[test]
fn parameter_values_include_calls_in_other_made_units() {
    pair(
        "function run(f: () => void): void { f(); }",
        "const dormant = (): void => { run(good); }; run(good);",
        "const dormant = (): void => { run(bad); }; run(good);",
        "`run` -> an indirect call",
    );
}

#[test]
fn escape_uses_in_uncalled_units_still_classify_made_values() {
    for use_site in [
        "const fs: (() => void)[] = [VALUE];",
        "save(VALUE);",
        "forward(VALUE);",
    ] {
        let source = |value: &str| {
            format!("function save(f: () => void): () => void {{ return f; }} function forward(f: () => void): () => void {{ return save(f); }} function dormant(): void {{ {} }}
const good = (): void => {{}}; const bad = (): void => {{ print(`${{LATER}}`); }}; let hook = good; hook(); const LATER: i32 = 5; export function main(): void {{}}", use_site.replace("VALUE", value))
        };
        check_program(&[SourceFile::entry("main.ts", source("good"))])
            .expect("only the safe value escapes");
        let errors = check_program(&[SourceFile::entry("main.ts", source("bad"))])
            .expect_err("the made value escapes at its unit use");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].message,
            "`LATER` is accessed before its declaration, through an indirect call"
        );
    }
}

#[test]
fn a_made_unit_supplies_known_calls_from_its_direct_closure() {
    pair(
        "function run(f: () => void): void { f(); } function helper(): void { run(bad); }",
        "const dormant = (): void => {}; run(good);",
        "const dormant = (): void => { helper(); }; run(good);",
        "`run` -> an indirect call",
    );
}

#[test]
fn an_assigned_parameter_runs_the_assigned_value() {
    let definitions =
        "function run(f: () => void): void { f = (): void => { print(`${LATER}`); }; f(); }";
    let global = "const LATER: i32 = 5;";
    let call = "run((): void => {});";
    let source = |early: bool| {
        let body = if early {
            format!("{call} {global}")
        } else {
            format!("{global} {call}")
        };
        format!("{definitions} {body} export function main(): void {{}}")
    };
    check_program(&[SourceFile::entry("main.ts", source(false))])
        .expect("the assigned value reads an initialized global");
    let errors = check_program(&[SourceFile::entry("main.ts", source(true))])
        .expect_err("the assigned value reads a later global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(
        errors[0].message,
        "`LATER` is accessed before its declaration, through `run` -> an indirect call"
    );
}

#[test]
fn an_assigned_parameter_escapes_the_forwarded_value() {
    pair(
        "function run(f: () => void, g: () => void): void { f = g; f(); }",
        "run(good, good);",
        "run(good, bad);",
        "`run` -> an indirect call",
    );
}

#[test]
fn an_assignment_classifies_the_parameter_for_the_whole_unit() {
    pair("function run(f: () => void, g: () => void, flag: boolean): void { if (flag) { f = g; } f(); }", "run(good, good, false);", "run(bad, good, false);", "`run` -> an indirect call");
    pair(
        "function run(f: () => void, g: () => void): void { const alias = f; f = g; alias(); }",
        "run(good, good);",
        "run(bad, good);",
        "`run` -> an indirect call",
    );
}

#[test]
fn assignments_to_shadowed_bindings_do_not_classify_the_outer_parameter() {
    pair(
        "function run(f: () => void): void { { let f = bad; f = good; } f(); }",
        "run(good);",
        "run(bad);",
        "`run` -> an indirect call",
    );
    pair("function run(f: () => void): void { let escaped = bad; const other = (f: () => void, g: () => void): void => { f = g; }; f(); }", "run(good);", "run(bad);", "`run` -> an indirect call");
}

#[test]
fn an_assigned_builtin_callback_parameter_runs_escaped_values() {
    let source = |read: &str| {
        format!("function mapped(xs: i32[], f: (x: i32) => i32): void {{ f = (x: i32): i32 => x + {read}; print(`${{xs.map(f).join(\",\")}}`); }} mapped([1], (x: i32): i32 => x); const LATER: i32 = 5; export function main(): void {{}}")
    };
    check_program(&[SourceFile::entry("main.ts", source("1"))])
        .expect("the assigned callback reads no later global");
    let errors = check_program(&[SourceFile::entry("main.ts", source("LATER"))])
        .expect_err("the assigned callback reads the later global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        "`LATER` is accessed before its declaration, through `mapped` -> an indirect call"
    );
}

#[test]
fn builtin_callback_inputs_include_escaped_values() {
    for (definitions, call, route) in [
        ("", "callbacks.forEach((h: () => void, k: string): void => { h(); });", "a lambda -> an indirect call"),
        ("function invoke(h: () => void, k: string): void { h(); }", "callbacks.forEach(invoke);", "`invoke` -> an indirect call"),
        ("function visit(m: Map<string, () => void>): void { m.forEach((h: () => void, k: string): void => { h(); }); }", "visit(callbacks);", "`visit` -> a lambda -> an indirect call"),
        ("function invoke(h: () => void, k: string): void { h(); } function visit(m: Map<string, () => void>): void { m.forEach(invoke); }", "visit(callbacks);", "`visit` -> `invoke` -> an indirect call"),
        ("function visit(m: Map<string, () => void>, f: (h: () => void, k: string) => void): void { m.forEach(f); }", "visit(callbacks, (h: () => void, k: string): void => { h(); });", "`visit` -> an indirect call"),
    ] {
        let prefix = format!("{definitions} const callbacks: Map<string, () => void> = new Map<string, () => void>();");
        pair(&prefix, &format!("callbacks.set(\"value\", good); {call}"), &format!("callbacks.set(\"value\", bad); {call}"), route);
    }
}

#[test]
fn builtin_callback_reject_entries_pin_the_full_route() {
    for source in [
        include_str!("../../corpus/reject/r552-a-map-calls-a-lambda-callback.ts"),
        include_str!("../../corpus/reject/r553-a-map-calls-a-declared-callback.ts"),
    ] {
        let expected = source
            .lines()
            .find_map(|line| line.strip_prefix("// expected-error: S100: "))
            .unwrap();
        let declaration = source
            .lines()
            .find(|line| line.starts_with("const LATER:"))
            .unwrap();
        let control = source.replacen(declaration, "", 1).replacen(
            "callbacks.forEach",
            &format!("{declaration}\ncallbacks.forEach"),
            1,
        );
        check_program(&[SourceFile::entry("main.ts", control)])
            .expect("the same callback reads an initialized global");
        let errors = check_program(&[SourceFile::entry("main.ts", source)])
            .expect_err("the callback reads the later global");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].message, expected);
    }
}
