//! Full text regressions for §303. Costs are measured with --test-threads=1.
use crate::divergence::{Divergence, DivergenceEntry};
use crate::{check_program, Diagnostic, SourceFile};

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let start = std::time::Instant::now();
    let result = check_program(&[SourceFile::entry("main.ts", source)]).expect_err("rejected");
    eprintln!("measured checker cost: {:?}", start.elapsed());
    result
}

#[test]
// Measured checker cost with --test-threads=1: 0.521 ms.
fn s303_rule1() {
    let ds = diagnostics(
        "let h = (a: i32, b: i32 = 5): i32 => a + b; h(1); export function main(): void {}",
    );
    assert_eq!(
        ds[0].message,
        "`h` expects 2 argument(s), got 1: a call through a function value passes every parameter"
    );
    assert_eq!(
        ds[0].note,
        Some("Pass every argument, or call a declaration with a default directly.")
    );
    assert_eq!(
        ds[0].divergence.unwrap().entry().why,
        "A function type has no optional parameter (C7)."
    );
}

#[test]
// Measured checker cost with --test-threads=1: 0.339 ms.
fn s303_rule2() {
    let ds = diagnostics("async function f(): Promise<i32> { return 1; } export async function main(): Promise<void> { await f().then((x: i32): i32 => x, null); }");
    assert_eq!(
        ds[0].message,
        "`.then(...)` takes no `null` rejection callback; use `.then(f)`"
    );
    assert_eq!(*ds[0].example.unwrap(), DivergenceEntry {
        ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { await leaf().then((x: i32): i32 => x, null); }",
        subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { await leaf().then((x: i32): i32 => x); }",
        why: "Omit a null rejection callback; use then(f).",
        collision: "collisions.md C8; compiler.md §186 rule 2",
    });
}

#[test]
// Measured checker cost with --test-threads=1: 0.319 ms.
fn s303_rule2_null_fulfillment_control() {
    let ds = diagnostics("async function f(): Promise<i32> { return 1; } export async function main(): Promise<void> { await f().then(null, (e: Error): i32 => 0); }");
    assert_eq!(
        ds[0].message,
        "`.then(...)` takes no `null` callback; `.catch(r)` takes a rejection callback alone"
    );
}

#[test]
// Measured checker cost with --test-threads=1: 0.353 ms.
fn s303_rule3() {
    let ds = diagnostics("async function f(): Promise<i32> { return 1; } export async function main(): Promise<void> { const fs: ((x: i32) => i32)[] = []; await f().then(...fs); }");
    assert_eq!(
        ds[0].message,
        "spread arguments require variadic parameters, which the language does not have"
    );
    assert_eq!(*ds[0].example.unwrap(), DivergenceEntry {
        ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const callbacks: ((x: i32) => i32)[] = [(x: i32): i32 => x]; await leaf().then(...callbacks); }",
        subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { await leaf().then((x: i32): i32 => x); }",
        why: "A reaction takes explicit callbacks; spread arguments require variadic parameters, which the language does not have.",
        collision: "collisions.md C8; compiler.md §186 rule 3",
    });
}

#[test]
// Measured checker cost with --test-threads=1: 0.291 ms.
fn s303_rule5_container() {
    let ds = diagnostics("export function main(): void { new Map<string, i32>().forEach((v: i32, k: string, all: Map<string, i32>): void => {}); }");
    assert_eq!(ds[0].message, "`Map.forEach` callbacks take exactly 2 parameter(s); extra lib callback parameters are not accepted (Q24)");
    assert_eq!(ds[0].divergence, Some(Divergence::CallbackParameterShape));
    assert_eq!(ds[0].example.copied().unwrap_or(ds[0].divergence.unwrap().entry()), DivergenceEntry {
        ts: "const m = new Map<string, i32>(); m.forEach((v: i32, k: string, all: Map<string, i32>): void => {});",
        subscript: "const m = new Map<string, i32>(); m.forEach((v: i32, k: string): void => {});",
        why: "A container callback takes its element parameters and an optional index; a parameter for the container itself is not passed.",
        collision: "stdlib.md §10.4",
    });
}

#[test]
// Measured checker cost with --test-threads=1: 0.242 ms.
fn s303_rule5_set_repeated_value() {
    let ds = diagnostics("export function main(): void { new Set<i32>().forEach((v: i32, again: i32): void => {}); }");
    assert_eq!(ds[0].message, "`Set.forEach` callbacks take exactly 1 parameter(s); extra lib callback parameters are not accepted (Q24)");
    assert_eq!(ds[0].divergence, Some(Divergence::CallbackParameterShape));
    assert_eq!(*ds[0].example.unwrap(), DivergenceEntry {
        ts: "new Set<i32>().forEach((v: i32, again: i32): void => {});",
        subscript: "new Set<i32>().forEach((v: i32): void => {});",
        why: "Set.forEach passes the value twice in JavaScript; this callback place passes it once.",
        collision: "stdlib.md §10.4; Q27",
    });
}

#[test]
// Measured checker cost with --test-threads=1: 1.704 ms total for seven cases (0.208–0.288 ms each).
fn s303_rule5a() {
    for (method, declared, actual, source) in [
        ("sort", 2, 3, "[1].sort((a: i32, b: i32, i: i32): i32 => a - b);"),
        ("map", 3, 4, "[1].map((v: i32, i: i32, a: i32[], extra: i32): i32 => v);"),
        ("reduce", 4, 5, "[1].reduce((acc: i32, v: i32, i: i32, a: i32[], extra: i32): i32 => acc + v, 0);"),
        ("Map.forEach", 3, 4, "new Map<string, i32>().forEach((v: i32, k: string, m: Map<string, i32>, extra: i32): void => {});"),
        ("Map.groupBy", 1, 2, "Map.groupBy([1], (v: i32, i: i32): i32 => v);"),
        ("map", 2, 4, "const xs: FixedArray<i32, 1> = [1]; xs.map((v: i32, i: i32, a: i32[], extra: i32): i32 => v);"),
        ("reduce", 3, 5, "const xs: FixedArray<i32, 1> = [1]; xs.reduce((acc: i32, v: i32, i: i32, a: i32[], extra: i32): i32 => acc + v, 0);"),
    ] {
        let ds = diagnostics(&format!("export function main(): void {{ {source} }}"));
        assert_eq!(ds[0].message, format!("`{method}` callback type declares {declared} parameter(s); got {actual}"));
        assert_eq!(ds[0].divergence, None);
        assert_eq!(ds[0].example, None);
    }
}

#[test]
// Measured checker cost with --test-threads=1: 0.504 ms.
fn s303_rule6() {
    let ds = diagnostics("function one(x: i32): void {} function three(x: i32, i: i32, a: i32[]): void {} export function main(): void { const b: boolean = true; [1].forEach(b ? one : three); }");
    assert_eq!(
        ds[0].message,
        "conditional branches have no common type: `(i32) => void` and `(i32, i32, i32[]) => void`"
    );
}

#[test]
// Measured checker cost with --test-threads=1: 1.190 ms total for four cases (0.272–0.329 ms each).
fn s303_rule8() {
    for usage in [
        "const x: i32 = B.z;",
        "B.z = 2;",
        "const x: i32 =\n B.z + B.z;",
        "B.z = B.z + 1;",
    ] {
        let ds = diagnostics(&format!(
            "{usage} class B {{ static z: i32 = 1; }} export function main(): void {{}}"
        ));
        let expected = if usage.matches("B.z").count() == 2 {
            2
        } else {
            1
        };
        assert_eq!(ds.len(), expected, "{ds:?}");
        for d in ds {
            assert_eq!(d.message, "`B` is used before its declaration");
        }
    }
}

#[test]
// Measured checker cost with --test-threads=1: 0.477 ms.
fn s303_rule8_other_site_control() {
    let ds = diagnostics("const x: i32 = B.z;\nconst y: i32 = read();\nfunction read(): i32 { return B.z; }\nclass B { static z: i32 = 1; }\nexport function main(): void {}");
    assert_eq!(
        ds.iter().map(|d| d.message.as_str()).collect::<Vec<_>>(),
        [
            "`B` is used before its declaration",
            "`B.z` is accessed before its declaration, through `read`",
        ]
    );
}
