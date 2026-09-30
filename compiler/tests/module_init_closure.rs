//! Item closures under compiler.md §137 rule 5b.
//! Cost: the scan unit test counts visits for twelve callbacks; no ship-C compiles.

use subscript_compiler::{check_program, RuleCode, SourceFile};

fn rejected(source: &str, message: &str) {
    let errors = check_program(&[SourceFile::entry("main.ts", source)])
        .expect_err("the item reads a later global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, message);
}

#[test]
fn a_loop_follows_values_made_in_a_later_iteration() {
    rejected("class Box { v: i32 = 1; } let f: () => i32 = (): i32 => 0; let x: i32 = 0; for (let i: i32 = 0; i < 2; i++) { x = f(); f = (): i32 => later.v; } const later: Box = new Box(); export function main(): void {}", "`later` is accessed before its declaration, through an indirect call");
}

#[test]
fn a_loop_closes_values_made_by_its_direct_body() {
    rejected("class Box { v: i32 = 1; } let f: () => i32 = (): i32 => 0; let x: i32 = 0; function step(): void { x = f(); f = (): i32 => later.v; } let i: i32 = 0; while (i < 2) { step(); i += 1; } const later: Box = new Box(); export function main(): void {}", "`later` is accessed before its declaration, through `step` -> an indirect call");
}

#[test]
fn a_generator_step_follows_values_made_after_creation() {
    rejected("class Box { v: i32 = 1; } let f: () => i32 = (): i32 => 0; function* gen(): Generator<i32> { yield 0; yield f(); } const g: Generator<i32> = gen(); const a: i32 = g.next().value; f = (): i32 => later.v; const b: i32 = g.next().value; const later: Box = new Box(); export function main(): void {}", "`later` is accessed before its declaration, through an indirect call");
}

fn twelve_callbacks(read_later: bool) -> String {
    let mut source = "const values: i32[] = [1]; function leaf(): i32 { return 1; } const stored: () => i32 = leaf;".to_string();
    for index in 0..12 {
        let body = if read_later && index == 11 {
            "later"
        } else {
            "stored()"
        };
        source.push_str(&format!("const cb{index}: () => i32 = (): i32 => {{ values.filter((v: i32): boolean => v > 0); return {body}; }};"));
    }
    source.push_str("const first: i32 = cb0(); const later: i32 = 1; export function main(): void { print(`${first}`); }");
    source
}

#[test]
fn twelve_callbacks_read_only_initialized_globals() {
    check_program(&[SourceFile::entry("main.ts", twelve_callbacks(false))])
        .expect("callbacks read only initialized globals");
    rejected(
        &twelve_callbacks(true),
        "`later` is accessed before its declaration, through an indirect call",
    );
}
