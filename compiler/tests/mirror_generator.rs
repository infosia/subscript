//! The C ABI has no generator frame (§314 rule 2a).
//! Cost: 13 checker calls; measured debug suite: 0.01 s.
use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn mirror_generator_types_are_rejected_in_every_position() {
    let mut failures = Vec::new();
    for declaration in [
        "declare function hostGenerator(): Generator<i32>;",
        "declare function take(value: Generator<i32>): void;",
        "type Held = Generator<i32>;",
        "type Held = Generator<i32>[];",
        "type Callback = (value: Generator<i32>) => void;",
        "type Callback = () => Generator<i32>;",
        "declare class Held { value: Generator<i32>; }",
        "declare class Held { constructor(value: Generator<i32>); }",
        "declare class Held { get(): Generator<i32>; }",
        "declare class Held { take(value: Generator<i32>): void; }",
        "type Held = FixedArray<Generator<i32>, 1>;",
        "declare function get(): Promise<Generator<i32>>;",
    ] {
        let mirror = format!("// @subscript-c-header include=\"host.h\"\n{declaration}\n");
        let diagnostics = check_program(&[
            SourceFile::ambient("host.d.ts", mirror),
            SourceFile::entry("main.ts", "export function main(): void {}"),
        ])
        .err()
        .unwrap_or_default();
        if !diagnostics.iter().any(|diagnostic| {
            diagnostic.code == RuleCode::S100
                && diagnostic.message.contains("C ABI has no generator")
                && diagnostic.divergence.is_some()
        }) {
            failures.push(format!("{declaration}: {diagnostics:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn script_generator_types_remain_accepted() {
    check_program(&[SourceFile::entry("main.ts", "function* gen(): Generator<i32> { yield 1; } export function main(): void { const g = gen(); print(`${g.next().value}`); }")])
        .expect("script generators register their frames");
}
