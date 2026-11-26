//! An in-place `filter` in the three tiers (`compiler.md` §359 rules 5
//! and 8).

use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::{lower_module, lower_module_with_in_place_mode, InPlaceMode},
    run_c_aot, run_jit, RunError,
};
use subscript_compiler::{check_program, SourceFile};

#[allow(dead_code)]
mod corpus;

/// The count of in-place `filter` end steps in the LIR of `source`.
fn end_steps(source: &str) -> usize {
    let files = [SourceFile::new("count.ts", source)];
    let hir = check_program(&files).expect("the program checks");
    let lir = lower_module(&hir).expect("the program lowers");
    lir.functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(&instruction.kind, subscript_compiler::lir::InstructionKind::Call(target)
                if target.kind == subscript_compiler::lir::CallTargetKind::BuiltinMethod(
                    subscript_compiler::lir::BuiltinMethod::ArrayTruncateKept))
        })
        .count()
}

/// The output, the trap rule, and the trap position of each tier, in the
/// order interpreter, dev JIT, ship C.
fn trapped_runs(name: &str, source: &str) -> Vec<(String, String, (u32, u32))> {
    let files = [SourceFile::new(format!("{name}.ts"), source)];
    let hir = check_program(&files).unwrap_or_else(|d| panic!("{name}: check: {d:?}"));
    let lir = lower_module(&hir).unwrap_or_else(|e| panic!("{name}: lower: {e:?}"));
    let interpreted = match interpret(&lir) {
        Err(InterpretError::Execution { source, output, .. }) => match *source {
            InterpretError::Trap { kind, pos, .. } => (
                String::from_utf8_lossy(&output).into(),
                kind,
                (pos.line, pos.col),
            ),
            other => panic!("{name}: interpreter: {other}"),
        },
        other => panic!("{name}: interpreter did not trap: {other:?}"),
    };
    let compiled = |result: Result<Vec<u8>, RunError>, tier: &str| match result {
        Err(RunError::Trap(report)) => (
            String::from_utf8_lossy(&report.stdout).into_owned(),
            report.rule.rule().to_owned(),
            (report.pos.line, report.pos.col),
        ),
        other => panic!("{name}: {tier} did not trap: {other:?}"),
    };
    vec![
        interpreted,
        compiled(run_jit(&files), "dev JIT"),
        compiled(run_c_aot(&files), "ship C"),
    ]
}

/// Rule 5: a callback that traps at an element in the middle gives the
/// output and the trap of a `filter` that allocates. `allocating` reads a
/// parameter, so its `filter` is not in place.
///
/// Cost: two C compiles and two runs of each other tier; 1 s.
#[test]
fn a_trap_in_the_callback_is_that_of_a_filter_that_allocates() {
    let program = |receiver: &str, call: &str| {
        format!(
            "function pick(values: i32[], stop: i32): i32[] {{ const at: i32 = stop; {receiver}
  return {call}.filter((x: i32, i: i32): boolean => {{
    print(`visit ${{x}}`);
    return 10 / (x - at) > 0;
  }});
}}
export function main(): void {{
  const xs: i32[] = [1, 2, 3, 4, 5];
  print(`${{pick(xs, 6).join(\",\")}}`);
  print(`${{pick(xs, 3).join(\",\")}}`);
}}
"
        )
    };
    let in_place_source = program(
        "const mapped: i32[] = values.map((x: i32): i32 => x);",
        "mapped",
    );
    let allocating_source = program("", "values");
    assert_eq!(end_steps(&in_place_source), 1);
    assert_eq!(end_steps(&allocating_source), 0);
    let in_place = trapped_runs("in-place", &in_place_source);
    let allocating = trapped_runs("allocating", &allocating_source);
    assert_eq!(in_place, allocating);
    let (output, rule, _) = &in_place[0];
    assert!(output.ends_with("visit 3\n"), "{output}");
    assert!(!rule.is_empty());
    assert!(
        in_place.iter().all(|run| run == &in_place[0]),
        "{in_place:?}"
    );
}

/// Rule 8: an in-place `filter` that removes strings and objects, then a
/// whole collection. The debug check of each whole collection traps if a
/// byte past the length of an array is not zero (§80 rule 3); its firing
/// control is the runtime test `array_tail_violations_reports_a_stale_word`.
/// With the `memset` of the end step removed from the emitted C, the ship
/// run traps with "array tail violations" (measured once).
///
/// Cost: one C compile and one run of each other tier; 0.5 s.
#[test]
fn a_whole_collection_after_the_end_step_finds_zero_tails() {
    let source = "class Box { value: i32; constructor(value: i32) { this.value = value; } }
export function main(): void {
  const xs: i32[] = [1, 2, 3, 4, 5, 6];
  const words: string[] = xs.map((x: i32): string => `word${x}`).filter((w: string): boolean => w.endsWith(\"2\"));
  const boxes: Box[] = xs.map((x: i32): Box => new Box(x)).filter((b: Box): boolean => b.value === 5);
  Context.collect();
  print(`${words.join(\",\")} ${boxes.length} ${boxes[0].value}`);
}
";
    assert_eq!(end_steps(source), 2, "both calls are in place");
    let files = [SourceFile::new("zero-tails.ts", source)];
    let hir = check_program(&files).expect("the program checks");
    let lir = lower_module(&hir).expect("the program lowers");
    let expected = b"word2 1 5\n";
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

/// True if `lir` has an in-place `filter`: a store of a kept element.
fn has_kept_store(lir: &subscript_compiler::lir::Module) -> bool {
    lir.functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(&instruction.kind, subscript_compiler::lir::InstructionKind::Call(target)
                if target.kind == subscript_compiler::lir::CallTargetKind::BuiltinMethod(
                    subscript_compiler::lir::BuiltinMethod::ArrayStoreKept))
        })
}

/// The interpreter output of `lir`, with the error that stopped it.
fn interpreted(lir: &subscript_compiler::lir::Module) -> (Vec<u8>, Option<String>) {
    match interpret(lir) {
        Ok(output) => (output, None),
        Err(error) => (error.output().to_vec(), Some(error.to_string())),
    }
}

/// `compiler.md` §359 rule 14 and acceptance 8a: each corpus accept entry
/// with an in-place `filter` prints the same output when each `filter`
/// allocates. The reference does not read the decision. The control
/// lowers the same entries with rule 3 ignored; it must report `a512`.
///
/// The interpreter runs each lowering: it executes LIR with no C compiler.
/// Only an entry whose source names `filter` can have an in-place call, so
/// the other entries are not checked.
///
/// Cost: one check and three lowerings and three interpreter runs of each
/// entry with an in-place `filter`, one check and one lowering of each
/// other entry that names `filter`; 1.2 s in a debug build.
#[test]
fn each_entry_with_an_in_place_filter_prints_the_output_of_a_filter_that_allocates() {
    let accept = corpus::corpus_accept();
    let mut ran = Vec::new();
    let mut differences = Vec::new();
    let mut control = Vec::new();
    for id in corpus::entry_ids(&accept) {
        let files = corpus::entry_sources(&accept, &id);
        if !files.iter().any(|file| file.source.contains("filter")) {
            continue;
        }
        let hir = corpus::check_program(&files).unwrap_or_else(|d| panic!("{id}: check: {d:?}"));
        let lower = |mode: InPlaceMode| {
            lower_module_with_in_place_mode(&hir, mode)
                .unwrap_or_else(|error| panic!("{id}: lower in {mode:?}: {error}"))
        };
        let normal = lower(InPlaceMode::Normal);
        if !has_kept_store(&normal) {
            continue;
        }
        let reference = interpreted(&lower(InPlaceMode::Off));
        if interpreted(&normal) != reference {
            differences.push(id.clone());
        }
        if interpreted(&lower(InPlaceMode::IgnoreRule3)) != reference {
            control.push(id.clone());
        }
        ran.push(id);
    }
    println!("rule 14 ran {} entries: {}", ran.len(), ran.join(" "));
    println!("rule 14 control reports: {}", control.join(" "));
    assert!(!ran.is_empty(), "no corpus entry has an in-place filter");
    assert!(
        differences.is_empty(),
        "the in-place form changes the output of {differences:?}"
    );
    assert_eq!(
        control,
        ["a512-in-place-filter"],
        "with rule 3 ignored, the test must report the loop case of a512"
    );
}
