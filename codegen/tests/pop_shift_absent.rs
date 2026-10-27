//! compiler.md §196.1: `pop()` and `shift()` on an empty array give
//! `undefined` for a `T` in the `Map.get` set, and trap for each other
//! `T`. The interpreter and both tiers agree.

use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::lower_module,
    run_c_aot, run_jit, RunError,
};
use subscript_compiler::{check_program, SourceFile};

/// The trap rule of each tier, in the order interpreter, dev JIT, ship C.
/// If `ship` is false, the test does not run the ship tier. Those cases
/// keep the trap of compiler.md §196.1 rule 2, and each tier calls the
/// same trapping runtime entry, so one ship run covers the entry.
fn trap_rules(name: &str, source: &str, ship: bool) -> Vec<String> {
    let files = [SourceFile::new(format!("{name}.ts"), source)];
    let hir = check_program(&files).unwrap_or_else(|d| panic!("{name}: check: {d:?}"));
    let lir = lower_module(&hir).unwrap_or_else(|e| panic!("{name}: lower: {e:?}"));
    let interpreted = match interpret(&lir) {
        Err(InterpretError::Execution { source, .. }) => match *source {
            InterpretError::Trap { kind, .. } => kind,
            other => panic!("{name}: interpreter: {other}"),
        },
        Err(InterpretError::Trap { kind, .. }) => kind,
        other => panic!("{name}: interpreter did not trap: {other:?}"),
    };
    let compiled = |result: Result<Vec<u8>, RunError>, tier: &str| match result {
        Err(RunError::Trap(report)) => report.rule.rule().to_owned(),
        other => panic!("{name}: {tier} did not trap: {other:?}"),
    };
    let mut rules = vec![interpreted, compiled(run_jit(&files), "dev JIT")];
    if ship {
        rules.push(compiled(run_c_aot(&files), "ship C"));
    }
    rules
}

/// A function element and an array element give `undefined` on an empty
/// array; a non-empty array gives its element.
///
/// Cost: one C compile, the only ship-tier run of the absent path for
/// these two element kinds.
#[test]
fn function_and_array_elements_give_undefined_in_three_tiers() {
    let source = r#"
function seven(): i32 { return 7; }
export function main(): void {
  const fs: (() => i32)[] = [seven];
  const f = fs.pop();
  print(`${f === undefined ? -1 : f()} ${fs.pop() === undefined} ${fs.shift() === undefined}`);
  const rows: i32[][] = [[1, 2]];
  const row = rows.shift();
  print(`${row === undefined ? -1 : row.length} ${rows.shift() === undefined} ${rows.pop() === undefined}`);
}
"#;
    let files = [SourceFile::new("absent.ts", source)];
    let hir = check_program(&files).expect("checked fixture");
    let lir = lower_module(&hir).expect("verified fixture");
    let expected = b"7 true true\n2 true true\n";
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

/// A value-class element and a `string` element keep the empty trap.
///
/// Cost: one C compile for the value-class `pop`; the other three cases
/// run the interpreter and the dev JIT only.
#[test]
fn value_class_and_string_elements_keep_the_empty_trap() {
    for (element, declaration, member, ship) in [
        (
            "Point",
            "@ValueType\nclass Point { x: i32 = 0; }\n",
            "pop",
            true,
        ),
        (
            "Point",
            "@ValueType\nclass Point { x: i32 = 0; }\n",
            "shift",
            false,
        ),
        ("string", "", "pop", false),
        ("string", "", "shift", false),
    ] {
        let source = format!(
            "{declaration}export function main(): void {{\n  const xs: {element}[] = [];\n  const r: {element} = xs.{member}();\n}}\n"
        );
        let rules = trap_rules(&format!("{element}-{member}"), &source, ship);
        assert!(
            rules.iter().all(|rule| rule == "empty-pop"),
            "{element}[].{member}(): {rules:?}"
        );
    }
}

/// A discarded `shift()` and `pop()` on a counted reference element
/// release the removed element, as `t87` and `t88` do on an async-handle
/// element.
///
/// Cost: two C compiles, one for each removal.
#[test]
fn discarded_counted_array_removals_release_the_element() {
    let shift = r#"
async function fail(): Promise<void> { throw new Error("lost"); }
async function use(skip: boolean): Promise<void> {
  const jobs: Promise<void>[][] = [[fail()]];
  if (skip) { jobs.shift(); }
  else { await jobs[0][0]; }
}
export async function main(): Promise<void> { await use(true); }
"#;
    let pop = r#"
async function fail(): Promise<void> { throw new Error("lost"); }
function remove(jobs: Promise<void>[][]): Promise<void>[] {
  const job: Promise<void>[] = jobs[0];
  jobs.pop();
  return job;
}
async function use(skip: boolean): Promise<void> {
  const jobs: Promise<void>[][] = [[fail()]];
  const removed: Promise<void>[] = remove(jobs);
  if (!skip) { await removed[0]; }
}
export async function main(): Promise<void> { await use(true); }
"#;
    for (name, source) in [("counted-shift", shift), ("counted-pop", pop)] {
        let rules = trap_rules(name, source, true);
        assert!(
            rules.iter().all(|rule| rule == "uncaught-exception"),
            "{name}: {rules:?}"
        );
    }
}
