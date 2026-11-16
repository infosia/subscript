//! Static readonly narrowing (compiler.md §322).

use crate::{check_program, SourceFile};

fn check(source: &str, accepted: bool) {
    let prefix = "class A { x: i32 = 1; f: A | null = null; } function mk(): A | null { return new A(); } function tick(): void {} ";
    let result = check_program(&[SourceFile::entry("main.ts", format!("{prefix}{source}"))]);
    if accepted {
        result.expect("the immutable static field keeps its fact");
    } else {
        let errors = result.expect_err("the fact does not reach this read");
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code.as_str(), "S011", "{source}: {errors:?}");
    }
}

/// Cost: 0.03 seconds for 15 checker programs; no engine subprocess.
#[test]
fn only_static_readonly_fields_keep_facts_across_effects() {
    let start = std::time::Instant::now();
    for (declaration, receiver, accepted) in [
        ("class K { static readonly R: A | null = mk(); }", "K", true),
        ("class K { static R: A | null = mk(); }", "K", false),
        ("class K { readonly R: A | null = mk(); }", "k", false),
    ] {
        for effect in [
            "tick();",
            "await pause();",
            "const h = new H(); h.R = null;",
            "for (let i: i32 = 0; i < 2; i++) { tick(); }",
            "try { tick(); } finally { tick(); }",
        ] {
            check(&format!("{declaration} class H {{ R: A | null = null; }} async function pause(): Promise<void> {{}} export async function main(): Promise<void> {{ const k = new K(); if ({receiver}.R !== null) {{ {effect} print(`${{{receiver}.R.x}}`); }} }}"), accepted);
        }
    }
    eprintln!("static readonly effects: {:?}", start.elapsed());
}

/// Cost: 0.014 seconds for six checker programs; no engine subprocess.
#[test]
fn static_readonly_lambda_facts_require_an_immediate_call() {
    let start = std::time::Instant::now();
    for (body, accepted) in [
        ("const read = (): i32 => K.R.x; print(`${read()}`);", false),
        ("print(`${((): i32 => K.R.x)()}`);", true),
        ("print(`${((): i32 => { tick(); return K.R.x; })()}`);", true),
        ("const read = (): i32 => { if (K.R === null) { return 0; } tick(); return K.R.x; }; print(`${read()}`);", true),
        ("((): void => { const read = (): i32 => K.R.x; })();", false),
        ("const read = (): i32 => ((): i32 => K.R.x)();", false),
    ] {
        check(&format!("class K {{ static readonly R: A | null = mk(); }} export function main(): void {{ if (K.R !== null) {{ {body} }} }}"), accepted);
    }
    eprintln!("static readonly lambdas: {:?}", start.elapsed());
}

/// Cost: 0.01 seconds for three checker programs; no engine subprocess.
#[test]
fn fields_below_static_readonly_storage_stay_shared() {
    let start = std::time::Instant::now();
    for (effect, accepted) in [("", true), ("tick();", false), ("K.R.f = null;", false)] {
        check(&format!("class K {{ static readonly R: A = new A(); }} export function main(): void {{ if (K.R.f !== null) {{ {effect} print(`${{K.R.f.x}}`); }} }}"), accepted);
    }
    eprintln!("static readonly descendant fields: {:?}", start.elapsed());
}

/// Cost: two checker programs; no engine subprocess.
#[test]
fn async_immediate_callees_drop_static_readonly_facts() {
    for effect in ["", "await pause();"] {
        check(&format!("class K {{ static readonly R: A | null = mk(); }} async function pause(): Promise<void> {{}} export async function main(): Promise<void> {{ if (K.R !== null) {{ await (async (): Promise<void> => {{ {effect} print(`${{K.R.x}}`); }})(); }} }}"), false);
    }
}
