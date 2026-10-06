//! Task groups require a unique lexical declaration (§170 rules 2 and 3).

use subscript_compiler::{check_program, RuleCode, SourceFile, Type};

const CONTROL: &str = "export async function main(): Promise<void> { const group: TaskGroup = new TaskGroup(); await group.join(); }";

#[test]
fn forbidden_group_positions_reject_with_a_lexical_control() {
    for source in [
        "let group: TaskGroup = new TaskGroup(); export function main(): void {}",
        "const group = new TaskGroup(); export function main(): void {}",
        "class Holder { group: TaskGroup; constructor(group: TaskGroup) { this.group = group; } } export function main(): void {}",
        "function make(): TaskGroup { return new TaskGroup(); } export function main(): void {}",
        "async function make(): Promise<TaskGroup> { return new TaskGroup(); } export function main(): void {}",
        "async function use(group: TaskGroup): Promise<void> {} export function main(): void {}",
        "type Groups = TaskGroup[]; export function main(): void {}",
        "function* use(group: TaskGroup): Generator<i32> { yield 1; } export function main(): void {}",
        "export async function main(): Promise<void> { let group = new TaskGroup(); await group.join(); }",
        "export async function main(): Promise<void> { const group = new TaskGroup(); const alias = group; await group.join(); }",
        "export async function main(): Promise<void> { const group = new TaskGroup(); const groups = [group]; await group.join(); }",
        "function assign(group: TaskGroup): void { group = group; } export function main(): void {}",
        "export async function main(): Promise<void> { const group = new TaskGroup(); const join = group.join; await group.join(); }",
        "export async function main(): Promise<void> { const group = new TaskGroup(); const join = group[\"join\"]; await group.join(); }",
        "export async function main(): Promise<void> { const group = new TaskGroup(); group.missing(); await group.join(); }",
        "export async function main(): Promise<void> { const group = new TaskGroup(); const f = (): void => { group.join(); }; await group.join(); }",
        "export async function main(): Promise<void> { const group = new TaskGroup(); print(`${group}`); await group.join(); }",
    ] {
        let errors = check_program(&[SourceFile::new("position.ts", source)]).expect_err(source);
        assert!(errors.iter().any(|error| error.code == RuleCode::S009), "{source}: {errors:?}");
        check_program(&[SourceFile::new("control.ts", CONTROL)]).expect("lexical control");
    }
}

#[test]
fn a_borrow_does_not_discharge_the_local_join_obligation() {
    for joined in [false, true] {
        let source = format!("function borrow(group: TaskGroup): void {{}} export async function main(): Promise<void> {{ const group = new TaskGroup(); borrow(group); {} }}", if joined { "await group.join();" } else { "" });
        let result = check_program(&[SourceFile::new("borrow.ts", source)]);
        if joined {
            result.expect("joined control");
        } else {
            assert!(result
                .expect_err("unjoined borrow")
                .iter()
                .any(|error| error.code == RuleCode::S013));
        }
    }
}

#[test]
fn synchronous_methods_borrow_groups_and_layout_has_no_count() {
    let source = "class Helper { add(group: TaskGroup): void {} } export async function main(): Promise<void> { const group = new TaskGroup(); const helper = new Helper(); helper.add(group); await group.join(); }";
    check_program(&[SourceFile::new("method.ts", source)]).expect("synchronous borrow");
    check_program(&[SourceFile::new("arrow.ts", "export async function main(): Promise<void> { const group = new TaskGroup(); const borrow = (g: TaskGroup): void => {}; borrow(group); await group.join(); }")]).expect("synchronous arrow borrow");
    assert_eq!(
        subscript_compiler::types::scalar_size_align(&Type::TaskGroup),
        Some((8, 8))
    );
    assert!(!Type::TaskGroup.carries_async_handle());
    assert!(Type::TaskGroup.contained_types().is_empty());
}

#[test]
fn generator_groups_reject_with_an_async_body_control() {
    let source = include_str!("../../corpus/reject/r386-task-group-generator-body.ts");
    let control = source
        .replace(
            "function* values(): Generator<i32>",
            "async function values(): Promise<void>",
        )
        .replace("yield 1;", "await Context.suspend();")
        .replace("consume(group.join());", "await group.join();")
        .replace(
            "for (const value of values()) { print(`${value}`); break; }",
            "try { await values(); } catch (e) { if (e instanceof Error) { print(e.message); } }",
        );
    check_program(&[SourceFile::new("control.ts", control)]).expect("async body control");
    for source in [source,
        "function* values(): Generator<i32> { new TaskGroup(); yield 1; } export function main(): void {}",
        "function* values(): Generator<i32> { const borrow = (group: TaskGroup): void => {}; yield 1; } export function main(): void {}",
        "function borrow(group: TaskGroup): void {} function* values(group: TaskGroup): Generator<i32> { borrow(group); yield 1; } export function main(): void {}",
        "function* values(): Generator<i32> { const borrow = (group: TaskGroup): void => {}; const group = new TaskGroup(); borrow(group); yield 1; group.join(); } export function main(): void {}",
    ] {
        let errors = check_program(&[SourceFile::new("generator.ts", source)]).expect_err(source);
        assert!(errors.iter().any(|error| error.code == RuleCode::S009), "{source}: {errors:?}");
    }
}
