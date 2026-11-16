//! Compiler contract §320. Costs are measured below, excluding the build.
use std::time::Instant;
use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLS: &str = r#"
function f(a: i32, b: i32 = 10): i32 { return a + b; }
function app(k: (a: i32) => i32): i32 { return k(1); }
const g = (a: i32, b: i32 = 20): i32 => a * b;
class C { static f(a: i32, b: i32 = 30): i32 { return a + b; } }
"#;

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("defaults.ts", format!("{DECLS}{body}"))])
}

/// Rules 1–3: each name and place shares the adapter.
/// Measured cost: 0.040s for the short checker programs, excluding the build.
#[test]
fn names_take_the_existing_adapter_in_each_place() {
    let start = Instant::now();
    for body in [
        "function test(): void { app(f); app(g); app(C.f); const h: (a: i32) => i32 = f; h(2); }",
        "function test(): void { const k: i32 = 5; const l = (a: i32, b: i32 = k): i32 => a + b; app(l); }",
        "function test(): void { const alias = f; const alias2 = alias; app(alias2); }",
        "const alias = f; const alias2 = alias; function test(): void { app(alias2); }",
        "function test(): void { const alias = g; app(alias); }",
        "function test(): void { const l = (a: i32, b: i32 = 3): i32 => a + b; const alias = l; app(alias); }",
        "function test(): void { let h: (a: i32) => i32 = f; h = g; }",
        "function test(): (a: i32) => i32 { return f; }",
        "class H { cb: (a: i32) => i32 = f; }",
        "function test(): void { const fs: ((a: i32) => i32)[] = [f, g]; const more: ((a: i32) => i32)[] = [...fs, f]; }",
        "function test(b: boolean): void { app(b ? f : g); }",
        "function test(): void { const cb: ((a: i32) => i32) | null = null; const h = cb ?? f; }",
        "function test(): void { let cb: ((a: i32) => i32) | null = null; cb ??= f; }",
        "function zero(a: i32 = 2, b: i32 = a + 1): i32 { return a + b; } function test(): void { const z: () => i32 = zero; z(); }",
        "function test(): void { const cb: (a: i32) => void = f; cb(1); }",
    ] {
        check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
    let module = check("function test(): void { const h: (a: i32) => i32 = f; }").unwrap();
    let test = module.functions.iter().find(|f| f.name == "test").unwrap();
    let hir::Stmt::Let { init, .. } = &test.body[0] else {
        panic!("binding")
    };
    assert_eq!(init.ty, Type::func(vec![Type::I32], Type::I32));
    let hir::ExprKind::Lambda {
        params,
        body,
        captures,
        ..
    } = &init.kind
    else {
        panic!("adapter")
    };
    assert_eq!(params.len(), 1);
    assert!(captures.is_empty());
    assert!(
        matches!(&body[0], hir::Stmt::Return { value: Some(hir::Expr {
        kind: hir::ExprKind::Call { callee: hir::Callee::Func(_), args }, ..
    }), .. } if args.len() == 1)
    );
    eprintln!("names and places: {:?}", start.elapsed());
}

/// Rules 1 and 4: incompatible types and a missing required argument keep
/// the mismatch; an inferred let names its cause and has its own example.
/// Measured cost: 0.010s for six short checker programs, excluding the build.
#[test]
fn excluded_values_and_incompatible_parameters_keep_their_diagnostics() {
    let start = Instant::now();
    let errors = check("function test(): void { let l = f; app(l); }").unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    let diagnostic = &errors[0];
    assert!(diagnostic
        .message
        .contains("function value `l` needs an adapter for this place"));
    assert!(diagnostic.divergence.is_some(), "{diagnostic:?}");
    let example = diagnostic.example.unwrap();
    assert!(example.ts.contains("let l = f; app(l)"));
    assert!(example.subscript.contains("app((a: i32): i32 => f(a))"));
    check_program(&[SourceFile::new("alternative.ts", example.subscript)]).unwrap();
    for body in [
        "function wrong(a: string, b: i32 = 10): i32 { return b; } function test(): void { app(wrong); }",
        "function wrong(a: string, b: i32 = 10): i32 { return b; } function test(): void { let l = wrong; app(l); }",
        "function needed(a: i32 = 10, b: i32): i32 { return a + b; } function test(): void { app(needed); }",
        "async function asyncF(a: i32, b: i32 = 10): Promise<i32> { return a + b; } function test(): void { const h: (a: i32) => Promise<i32> = asyncF; }",
    ] {
        let errors = check(body).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert!(errors[0].message.starts_with("type mismatch:"), "{body}: {errors:?}");
    }
    eprintln!("diagnostic controls: {:?}", start.elapsed());
}

/// Rule 3: a const copy of a selected value still has no known defaults.
/// Measured cost: 0.037s for six short checker programs, excluding the build.
#[test]
fn a_const_copy_of_a_selected_value_keeps_the_rule_four_diagnostic() {
    let start = Instant::now();
    for body in [
        "function test(flag: boolean): void { let l = flag ? f : g; const saved = l; app(saved); }",
        "function test(flag: boolean): void { let l = flag ? f : g; const saved = l; const again = saved; app(again); }",
        "function test(flag: boolean): void { const saved = flag ? f : g; app(saved); }",
        "function test(flag: boolean): void { const saved = flag ? f : f; app(saved); }",
        "function indexed(a: i32, i: i32 = 10, b: i32 = 20): i32 { return a + i + b; } function test(): void { let l = indexed; [1].sort(l); }",
        "function indexed(a: i32, i: i32 = 10, b: i32 = 20): i32 { return a + i + b; } function test(): void { let l = indexed; const saved = l; [1].sort(saved); }",
    ] {
        let errors = check(body).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert!(errors[0].message.contains("defaults are not known at check time"), "{body}: {errors:?}");
        assert!(errors[0].message.contains("write a lambda that calls"), "{body}: {errors:?}");
        assert!(!errors[0].message.contains("copy it into a const local"), "{body}: {errors:?}");
        assert!(errors[0].divergence.is_some(), "{body}: {errors:?}");
        assert!(errors[0].example.is_some(), "{body}: {errors:?}");
    }
    eprintln!("selected value controls: {:?}", start.elapsed());
}

/// Rule 4a: the method supplies the index before defaults after it.
/// Measured cost: 0.037s for four short checker programs, excluding the build.
#[test]
fn builtin_callbacks_omit_only_defaults_after_the_supplied_arguments() {
    let start = Instant::now();
    for body in [
        "function test(): void { [1, 2].map(f); }",
        "function indexed(a: i32, i: i32 = 10, b: i32 = 20): i32 { return a + i + b; } function test(): void { [1, 2].sort(indexed); }",
        "function grp(a: i32, i: i32 = 99): string { return `${a}:${i}`; } function test(): void { Map.groupBy([5, 6], grp); }",
        "function test(): void { Map.groupBy([5, 6], (a: i32, i: i32 = 99): string => `${a}:${i}`); }",
    ] {
        check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
    eprintln!("built-in controls: {:?}", start.elapsed());
}

/// Rule 6: omitting a default cannot hide a TypeScript callback type error.
/// Measured cost: 0.042s for eight short checker programs, excluding the build.
#[test]
fn incompatible_library_parameters_stay_rejected_even_with_defaults() {
    let start = Instant::now();
    for body in [
        "function bad(a: i32, i: i32, xs: i32 = 0): i32 { return a; } function test(): void { [1].map(bad); }",
        "function bad(a: i32, i: i32, xs: i32 = 0): void {} function test(): void { [1].forEach(bad); }",
        "function bad(a: i32, i: i32, xs: i32 = 0): i32 { return a; } function test(): void { let l = bad; [1].map(l); }",
        "function bad(v: i32, k: i32, m: i32 = 0): void {} function test(): void { new Map<i32, i32>().forEach(bad); }",
        "function bad(v: i32, k: string = \"x\"): void {} function test(): void { new Set<i32>().forEach(bad); }",
        "function bad(v: i32, i: string = \"x\"): i32 { return v; } function test(): void { Map.groupBy([1], bad); }",
        "function required(v: i32, i: i32): i32 { return v + i; } function test(): void { Map.groupBy([1], required); }",
        "function test(): void { Map.groupBy([1], (v: i32, i: i32): i32 => v + i); }",
    ] {
        assert!(check(body).is_err(), "{body}");
    }
    eprintln!("library parameter controls: {:?}", start.elapsed());
}

/// Rule 4a: defaults at excluded library positions keep the pin diagnostic.
/// Measured cost: 0.038s for six short checker programs, excluding the build.
#[test]
fn filled_library_positions_keep_the_pin_diagnostic() {
    let start = Instant::now();
    for (body, message) in [
        ("function set(a: i32, b: i32 = 10): void {} function test(): void { const s = new Set<i32>(); s.forEach(set); }", "`Set.forEach` callbacks take exactly 1 parameter(s)"),
        ("function set(a: i32, b: i32 = 10, s: Set<i32> = new Set<i32>()): void {} function test(): void { new Set<i32>().forEach(set); }", "extra lib callback parameters are not accepted (Q24)"),
        ("function map(v: i32, k: i32, m: Map<i32, i32> = new Map<i32, i32>()): void {} function test(): void { new Map<i32, i32>().forEach(map); }", "extra lib callback parameters are not accepted (Q24)"),
        ("function ix(a: i32, i: i32 = 50, xs: i32[] = [9]): i32 { return a + i + xs.length; } function test(): void { [1, 2].map(ix); }", "callback with the container parameter"),
        ("function ix(a: i32, i: i32, xs: i32[] = [9], k: i32 = 1): i32 { return a; } function test(): void { [1, 2].map(ix); }", "callback type declares 3 parameter(s); got 4"),
        ("function set(a: i32, b: i32 = 10): void {} function test(): void { const alias = set; new Set<i32>().forEach(alias); }", "`Set.forEach` callbacks take exactly 1 parameter(s)"),
    ] {
        let errors = check(body).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert!(errors[0].message.contains(message), "{body}: {errors:?}");
    }
    eprintln!("filled library positions: {:?}", start.elapsed());
}

/// Rule 4a: FixedArray fills the same positions as Array at run time.
/// Cost: short checker programs, with no code generation or external process.
#[test]
fn fixed_array_filled_positions_keep_the_q27_container_diagnostic() {
    let start = Instant::now();
    for method in [
        "forEach",
        "map",
        "filter",
        "some",
        "every",
        "findIndex",
        "reduce",
        "reduceRight",
    ] {
        let reduce = matches!(method, "reduce" | "reduceRight");
        let predicate = matches!(method, "filter" | "some" | "every" | "findIndex");
        let ret = if predicate { "boolean" } else { "i32" };
        let result = if predicate { "true" } else { "v" };
        let prefix = if reduce { "acc: i32, " } else { "" };
        let init = if reduce { ", 0" } else { "" };
        for container in ["arr: i32[] = [9]", "z: i32 = 5"] {
            let callback = format!(
                "({prefix}v: i32, i: i32 = 1, {container}): {ret} => {{ return {result}; }}"
            );
            for declaration in [
                format!("const cb = {callback};"),
                format!("function cb({prefix}v: i32, i: i32 = 1, {container}): {ret} {{ return {result}; }}"),
                format!("const cb = {callback}; const alias = cb;"),
            ] {
                let name = if declaration.contains("const alias") { "alias" } else { "cb" };
                for callback in [name.to_string(), format!("flag ? {name} : {name}"), format!("flag ? (flag ? {name} : {name}) : {name}")] {
                    let body = format!("{declaration} function test(flag: boolean): void {{ const fa: FixedArray<i32, 2> = [1, 2]; fa.{method}({callback}{init}); }}");
                    let errors = check(&body).unwrap_err();
                    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
                    let diagnostic = &errors[0];
                    assert_eq!(diagnostic.code, RuleCode::S014, "{body}: {diagnostic:?}");
                    assert!(diagnostic.message.contains(&format!("`{method} callback with the container parameter`")), "{body}: {diagnostic:?}");
                    for text in ["C5", "container reference", "(Q27)"] {
                        assert!(diagnostic.message.contains(text), "{body}: {diagnostic:?}");
                    }
                }
            }
        }
    }
    let errors = check("const lf = (v: i32, i: i32 = 1, arr: i32[] = [9]): void => { print(`${arr.length}`); }; export function main(): void { const fa: FixedArray<i32, 2> = [1, 2]; fa.forEach(lf); }").unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0]
            .message
            .contains("`forEach callback with the container parameter`"),
        "{errors:?}"
    );
    // The third reduce parameter is the index. The pin accepts it.
    for method in ["reduce", "reduceRight"] {
        check(&format!("const fe = (v: i32, i: i32 = 1, z: i32 = 5): i32 => v + i + z; function test(): void {{ const fa: FixedArray<i32, 2> = [1, 2]; fa.{method}(fe, 0); }}")).unwrap();
    }
    eprintln!("FixedArray filled positions: {:?}", start.elapsed());
}

/// Rule 4a: a conditional keeps excluded arguments before the adapter runs.
/// Cost: short checker programs, with no code generation or external process.
#[test]
fn conditional_callbacks_keep_excluded_runtime_arguments() {
    for body in [
        "function cb(v: i32, i: i32 = 1, arr: i32[] = [9]): i32 { return v; } function test(flag: boolean): void { [1].map(flag ? cb : cb); }",
        "function cb(v: i32, i: i32 = 1, arr: i32[] = [9]): void {} function test(flag: boolean): void { [1].forEach(flag ? cb : cb); }",
        "function cb(v: i32, k: i32, m: Map<i32, i32> = new Map<i32, i32>()): void {} function test(flag: boolean): void { new Map<i32, i32>().forEach(flag ? cb : cb); }",
        "function cb(v: i32, k: i32 = 5): void {} function test(flag: boolean): void { new Set<i32>().forEach(flag ? cb : cb); }",
        "function cb(v: i32, k: i32 = 5, s: Set<i32> = new Set<i32>()): void {} function test(flag: boolean): void { new Set<i32>().forEach(flag ? cb : cb); }",
    ] {
        let errors = check(body).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S014, "{body}: {errors:?}");
        assert!(errors[0].message.contains("(Q27)") || errors[0].message.contains("(Q24)"), "{body}: {errors:?}");
    }
    for body in [
        "function cb(a: i32, b: i32, extra: i32 = 5): i32 { return a - b; } function test(flag: boolean): void { [1].sort(flag ? cb : cb); }",
        "function cb(v: i32): i32 { return v; } function test(flag: boolean): void { const fa: FixedArray<i32, 2> = [1, 2]; fa.map(flag ? cb : (): i32 => 5); }",
    ] {
        check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
}

/// An inferred callback result still rejects incompatible nested branches.
/// Cost: one checker program, with no code generation or external process.
#[test]
fn inferred_callback_results_reject_incompatible_nested_branches() {
    let errors = check("function n(v: i32): i32 { return v; } function s(v: i32): string { return `${v}`; } function test(a: boolean, b: boolean): void { const fa: FixedArray<i32, 2> = [1, 2]; fa.map(a ? (b ? n : s) : n); }").unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100, "{errors:?}");
}
