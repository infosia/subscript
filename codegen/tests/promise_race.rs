//! `Promise.race` in the three tiers (`compiler.md` §350 rules 3 to 7, 10,
//! 11, and 12; acceptance 5).
//!
//! Each program runs in the interpreter, the dev JIT, and the ship tier,
//! and each tier gives the expected text. The shapes share few programs,
//! so the file needs eleven ship-tier C builds. Measured: 1.0 s in a
//! release build and 1.7 s in a debug build, after the first C build of
//! the binary.

use std::path::PathBuf;

use subscript_codegen::interpreter::{interpret, interpret_with_collection, InterpretError};
use subscript_codegen::lir::lower_module;
use subscript_codegen::{
    run_c_aot, run_c_aot_configured, run_jit, run_jit_configured, NativeLibrary, RunConfig,
    RunError,
};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::context::CollectionMode;
use subscript_runtime::TrapKind;

/// The minimum headroom of the collection runs.
const MIN_HEADROOM: usize = 64 << 10;

fn files(source: &str) -> [SourceFile; 1] {
    [SourceFile::new("promise-race.ts", source)]
}

/// Runs `source` in the three tiers and compares each output with `expected`.
fn compare(source: &str, expected: &str) {
    let files = files(source);
    let lir = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let interpreter = interpret(&lir).unwrap_or_else(|error| panic!("interpreter: {error}"));
    assert_eq!(
        String::from_utf8_lossy(&interpreter),
        expected,
        "interpreter"
    );
    let dev = run_jit(&files).unwrap_or_else(|error| panic!("dev JIT: {error:?}"));
    assert_eq!(String::from_utf8_lossy(&dev), expected, "dev JIT");
    let ship = run_c_aot(&files).unwrap_or_else(|error| panic!("ship tier: {error:?}"));
    assert_eq!(String::from_utf8_lossy(&ship), expected, "ship tier");
}

/// The trap of `source` in each tier: output before the trap, the runtime
/// kind, the line, and the column.
fn traps(source: &str) -> [(String, TrapKind, u32, u32); 3] {
    let files = files(source);
    let lir = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let error = interpret(&lir).expect_err("the interpreter traps");
    let output = String::from_utf8_lossy(error.output()).into_owned();
    let InterpretError::Execution { source, .. } = error else {
        panic!("the interpreter keeps its output: {error}");
    };
    let InterpretError::Trap {
        runtime_kind: Some(kind),
        pos,
        ..
    } = *source
    else {
        panic!("the interpreter must trap: {source}");
    };
    let native = |tier: &str, run: Result<Vec<u8>, RunError>| match run {
        Err(RunError::Trap(report)) => (
            String::from_utf8_lossy(&report.stdout).into_owned(),
            report.rule,
            report.pos.line,
            report.pos.col,
        ),
        other => panic!("the {tier} must trap: {other:?}"),
    };
    [
        (output, kind, pos.line, pos.col),
        native("dev JIT", run_jit(&files)),
        native("ship tier", run_c_aot(&files)),
    ]
}

/// The shapes of rules 3, 4, 5, and 12, each result type, the held form,
/// the array value form, and a race in a `TaskGroup`. The turn counter
/// `turn` advances once for each job of `counter`. A direct `await` of a
/// task resumes 3 jobs after its start; an `await` of a race or of
/// `Promise.all` over the same task resumes one job later (rule 4). The
/// lines up to `turns` equal the output of node v24.22.1 for the same
/// program with its types stripped.
const SHAPES: &str = r#"
class Box { n: i32; constructor(n: i32) { this.n = n; } }
@ValueType
class Pt { x: f64 = 0.0; y: i32 = 0; constructor(x: f64, y: i32) { this.x = x; this.y = y; } }
let turn: i32 = 0;
async function nothing(): Promise<void> { return; }
async function wait(turns: i32): Promise<void> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
}
async function counter(): Promise<void> {
  for (let k: i32 = 0; k < 40; k += 1) { turn += 1; await nothing(); }
}
async function value(n: i32, turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
  return n;
}
async function fails(message: string, turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
  throw new Error(message);
}
async function flag(v: boolean, turns: i32): Promise<boolean> { await wait(turns); return v; }
async function real(v: f64, turns: i32): Promise<f64> { await wait(turns); return v; }
async function boxed(n: i32, turns: i32): Promise<Box> { await wait(turns); return new Box(n); }
async function maybe(n: i32, turns: i32): Promise<Box | null> {
  await wait(turns);
  return n < 0 ? null : new Box(n);
}
async function text(s: string, turns: i32): Promise<string> { await wait(turns); return `${s}!`; }
async function point(x: f64, turns: i32): Promise<Pt> { await wait(turns); return new Pt(x, 3); }
async function step(tag: string, turns: i32): Promise<void> { await wait(turns); print(`step ${tag}`); }
async function grouped(): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  group.add(Promise.race([step("g1", 2), step("g2", 1)]));
  await group.join();
}
export async function main(): Promise<void> {
  print(`first ${await Promise.race([value(1, 3), value(2, 1)])}`);
  try { await Promise.race([value(1, 3), fails("first", 1)]); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  const lose: Promise<i32> = fails("late", 4);
  print(`lost ${await Promise.race([value(5, 0), lose])}`);
  const a: Promise<i32> = value(7, 0);
  const b: Promise<i32> = value(8, 0);
  print(`order ${await Promise.race([a, b])} ${await Promise.race([b, a])}`);
  const pending: Promise<i32> = value(21, 1);
  const done: Promise<i32> = value(22, 0);
  print(`same turn ${await Promise.race([pending, done])}`);
  const slow: Promise<i32> = value(10, 2);
  print(`loser ${await Promise.race([slow, value(11, 0)])} ${await slow}`);
  const c: Promise<void> = counter();
  const s0: i32 = turn;
  await value(0, 2);
  const n0: i32 = turn - s0;
  const s1: i32 = turn;
  await Promise.race([value(0, 2)]);
  const n1: i32 = turn - s1;
  const s2: i32 = turn;
  await Promise.all([value(0, 2)]);
  const n2: i32 = turn - s2;
  print(`turns ${n0} ${n1} ${n2}`);
  await c;
  await Promise.race([step("v1", 2), step("v2", 1)]);
  print(`bool ${await Promise.race([flag(false, 2), flag(true, 1)])}`);
  print(`f64 ${await Promise.race([real(1.5, 2), real(2.25, 1)])}`);
  print(`class ${(await Promise.race([boxed(4, 2), boxed(5, 1)])).n}`);
  const m: Box | null = await Promise.race([maybe(6, 2), maybe(-1, 1)]);
  print(`nullable ${m === null ? "null" : "box"}`);
  const m2: Box | null = await Promise.race([maybe(7, 1), maybe(-1, 2)]);
  print(`nullable ${m2 === null ? "null" : `${m2.n}`}`);
  print(`string ${await Promise.race([text("a", 2), text("b", 1)])}`);
  const p: Pt = await Promise.race([point(1.25, 2), point(2.5, 1)]);
  print(`point ${p.x} ${p.y}`);
  const held: Promise<i32> = Promise.race([value(8, 2), value(9, 1)]);
  await wait(4);
  print(`held ${await held}`);
  const jobs: Promise<i32>[] = [value(10, 3), value(11, 2)];
  print(`array ${await Promise.race(jobs)} ${await jobs[0]}`);
  const twice: Promise<i32> = value(12, 1);
  print(`duplicate ${await Promise.race([twice, twice])}`);
  await grouped();
  print("end");
}
"#;

const SHAPES_EXPECTED: &str = "first 2\ncaught first\nlost 5\norder 7 8\nsame turn 22\n\
loser 11 10\nturns 3 4 4\nstep v2\nstep v1\nbool true\nf64 2.25\nclass 5\nnullable null\n\
nullable 7\nstring b!\npoint 2.5 3\nheld 9\narray 11 10\nduplicate 12\nstep g2\nstep g1\nend\n";

/// Rules 3 to 5 and 12 with each result type, in the three tiers. The
/// input `lose` fails after it lost; the run ends with no trap. Cost: one
/// ship-tier C build, one dev JIT compile, and one interpreter run.
#[test]
fn race_shapes_match_in_the_three_tiers() {
    compare(SHAPES, SHAPES_EXPECTED);
}

/// The control of rule 5: the same failed task with no race is held, not
/// awaited, and traps at its release at the end of `main`. Cost: one
/// ship-tier C build, one dev JIT compile, and one interpreter run.
#[test]
fn a_failed_task_with_no_race_traps_at_its_release() {
    const CONTROL: &str = r#"
async function nothing(): Promise<void> { return; }
async function fails(turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
  throw new Error("late");
}
let never: boolean = false;
export async function main(): Promise<void> {
  const lose: Promise<i32> = fails(2);
  for (let i: i32 = 0; i < 4; i += 1) { await nothing(); }
  print("end");
  if (never) { await lose; }
}
"#;
    for trap in traps(CONTROL) {
        assert_eq!(trap, ("end\n".into(), TrapKind::UncaughtException, 5, 3));
    }
}

/// Rule 7: a race over an empty `Promise<i32>[]` value traps with kind 38
/// at the call, after the output before it. Cost: one ship-tier C build,
/// one dev JIT compile, and one interpreter run.
#[test]
fn an_empty_race_traps_at_the_call_in_the_three_tiers() {
    const EMPTY: &str = r#"
export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [];
  print("before");
  const r: i32 = await Promise.race(jobs);
  print(`${r}`);
}
"#;
    for trap in traps(EMPTY) {
        assert_eq!(trap, ("before\n".into(), TrapKind::PromiseRaceEmpty, 5, 24));
        assert_eq!(trap.1 as u32, 38);
        assert_eq!(trap.1.rule(), "promise-race-empty");
    }
}

/// Rules 4 and 6 with a race that loses its last holder while its reaction
/// jobs are queued. In `run(true)` an exception leaves the scope before the
/// jobs run, so the scope releases the race; the job of the winner then
/// completes the race and releases it, and the job of the input that lost
/// still runs. `__LATE__` is the second input: a task with a completion at
/// the call, or a task that completes after the call and before the job of
/// the winner runs.
const RELEASED: &str = r#"
async function nothing(): Promise<void> { return; }
async function value(n: i32): Promise<i32> { return n; }
async function late(n: i32): Promise<i32> { await nothing(); return n; }
async function run(bad: boolean): Promise<void> {
  const b: Promise<i32> = __LATE__;
  const a: Promise<i32> = value(1);
  const r: Promise<i32> = Promise.race([a, b]);
  if (bad) { throw new Error("x"); }
  print(`${await r} ${await a} ${await b}`);
}
export async function main(): Promise<void> {
  try { await run(true); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  await nothing();
  await run(false);
  print("end");
}
"#;

/// The released race of [`RELEASED`] with both inputs that have a
/// completion at the call, in the three tiers: no trap and no internal
/// error. Cost: one ship-tier C build, one dev JIT compile, and one
/// interpreter run.
#[test]
fn a_released_race_with_queued_jobs_ends_in_the_three_tiers() {
    compare(
        &RELEASED.replace("__LATE__", "value(2)"),
        "caught x\n1 1 2\nend\n",
    );
}

/// The released race of [`RELEASED`] with an input that lost and
/// completes after the call, before the job of the winner runs. Cost: one
/// ship-tier C build, one dev JIT compile, and one interpreter run.
#[test]
fn a_released_race_with_a_loser_that_completes_late_ends_in_the_three_tiers() {
    compare(
        &RELEASED.replace("__LATE__", "late(2)"),
        "caught x\n1 1 2\nend\n",
    );
}

/// An exception wins a race that has no holder, with the job of the input
/// that lost still queued: the release of the race at the end of the job
/// of the winner traps at the `throw` of the winner, before the `catch` of
/// `main` prints, in the three tiers. Cost: one
/// ship-tier C build, one dev JIT compile, and one interpreter run.
#[test]
fn a_released_race_that_an_exception_wins_traps_in_the_three_tiers() {
    const FAILED: &str = r#"
async function value(n: i32): Promise<i32> { return n; }
async function fails(): Promise<i32> { throw new Error("won"); }
async function run(bad: boolean): Promise<void> {
  const a: Promise<i32> = fails();
  const b: Promise<i32> = value(2);
  const r: Promise<i32> = Promise.race([a, b]);
  if (bad) { throw new Error("x"); }
  print(`${await r}`);
}
export async function main(): Promise<void> {
  try { await run(true); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  print("end");
}
"#;
    for trap in traps(FAILED) {
        assert_eq!(trap, (String::new(), TrapKind::UncaughtException, 3, 40));
    }
}

/// Acceptance 5: each turn races a task that completes against the handle
/// of a pair of tasks that wait for each other and never complete.
/// `__FORM__` is the race or the `Promise.all` control.
const DETACH: &str = r#"
async function idle(): Promise<i32> { return 0; }
class Pair {
  first: Promise<i32>;
  second: Promise<i32>;
  constructor() { this.first = idle(); this.second = idle(); }
  start(): void { this.first = this.waitSecond(); this.second = this.waitFirst(); }
  async waitSecond(): Promise<i32> { await Context.suspend(); return await this.second; }
  async waitFirst(): Promise<i32> { await Context.suspend(); return await this.first; }
}
async function value(n: i32): Promise<i32> { return n; }
let never: boolean = false;
export async function main(): Promise<void> {
  const p: Pair = new Pair();
  p.start();
  let sum: i32 = 0;
  const alls: Promise<i32[]>[] = [];
  for (let i: i32 = 0; i < __N__; i += 1) {
    __FORM__
  }
  if (never) { for (const h of alls) { await h; } }
  Context.collect();
  print(`${sum > 0 ? "race" : "all"} ${alls.length > 0 ? "held" : "none"}`);
}
"#;

const RACE_FORM: &str = "sum += await Promise.race([value(1), p.first]);";
const ALL_FORM: &str = "alls.push(Promise.all([value(1), p.first]));";

/// The two repeat counts of acceptance 5.
const REPEATS: [u32; 2] = [1000, 1400];

fn temporary_directory() -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "subscript-promise-race-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).expect("create scratch directory");
    directory
}

/// The ship tier with the collection mode `mode` set before the
/// initializer. A post-run hook collects, then prints live bytes. Returns
/// the program output and live bytes.
fn ship(source: &str, mode: i32) -> (String, u64) {
    let scratch = temporary_directory();
    let hooks = scratch.join("race_hooks.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             void subscript_test_race_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, {mode}, {MIN_HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             void subscript_test_race_report(subscript_rt_context* ctx) {{\n\
               subscript_rt_ctx_collect(ctx);\n\
               printf(\"live-bytes=%llu\\n\", (unsigned long long)subscript_rt_ctx_live_bytes(ctx));\n\
               fflush(stdout);\n\
             }}\n"
        ),
    )
    .expect("write hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only the two host hooks; no script calls it.
    let library = [unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hooks], Vec::new())
    }];
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_race_setup");
    config.post_run_hook = Some("subscript_test_race_report");
    let output = run_c_aot_configured(&files(source), config).expect("ship run");
    let _ = std::fs::remove_dir_all(&scratch);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    let mut live = None;
    let mut program = String::new();
    for line in text.lines() {
        match line.strip_prefix("live-bytes=") {
            Some(value) => live = Some(value.parse::<u64>().expect("live bytes")),
            None => {
                program.push_str(line);
                program.push('\n');
            }
        }
    }
    (program, live.expect("the post-run hook printed live bytes"))
}

/// The dev JIT with the collection mode `mode`: output and live bytes.
fn dev(source: &str, mode: CollectionMode) -> (String, u64) {
    let mut config = RunConfig::default();
    config.memory_accounting = true;
    config.collection = Some((mode, MIN_HEADROOM));
    let output = run_jit_configured(&files(source), config).expect("dev run");
    let accounting = output.memory_accounting.expect("dev accounting");
    (
        String::from_utf8(output.stdout).expect("utf-8 output"),
        accounting.live_bytes,
    )
}

/// The interpreter with the collection mode `mode`: output and live bytes.
fn interpreted(source: &str, mode: CollectionMode) -> (String, u64) {
    let module = lower_module(&check_program(&files(source)).expect("checks")).expect("lowers");
    let (output, run) =
        interpret_with_collection(&module, mode, MIN_HEADROOM).expect("interpreter run");
    (
        String::from_utf8(output).expect("utf-8 output"),
        run.live_bytes as u64,
    )
}

/// The live bytes of `form` at the two counts of [`REPEATS`] in the dev
/// JIT, the ship tier, and the interpreter, with no automatic collection.
/// Each tier gives `expected`. Each tier compares live bytes with itself
/// only (`compiler.md` §18.2d).
fn live_at_two_counts(form: &str, expected: &str) -> [[u64; 2]; 3] {
    let mut figures = [[0; 2]; 3];
    for (index, repeats) in REPEATS.into_iter().enumerate() {
        let source = DETACH
            .replace("__N__", &repeats.to_string())
            .replace("__FORM__", form);
        let runs = [
            dev(&source, CollectionMode::Off),
            ship(&source, 0),
            interpreted(&source, CollectionMode::Off),
        ];
        for (tier, (output, live)) in runs.into_iter().enumerate() {
            assert_eq!(output, expected, "tier {tier}, {repeats} repeats");
            figures[tier][index] = live;
        }
    }
    println!(
        "{form}: dev JIT {:?}, ship tier {:?}, interpreter {:?}",
        figures[0], figures[1], figures[2]
    );
    figures
}

/// Acceptance 5 (rule 6): a race detaches from the input that lost, so live
/// bytes after the run are equal at the two counts in each tier. Control:
/// each `Promise.all` of the same inputs stays registered on the pair, so
/// live bytes grow with the count. Cost: four ship-tier C builds, four dev
/// JIT compiles, and four interpreter runs.
#[test]
fn a_race_per_turn_over_a_handle_that_never_completes_stays_bounded() {
    let [dev, ship, interpreter] = live_at_two_counts(RACE_FORM, "race none\n");
    assert_eq!(dev[0], dev[1], "dev JIT");
    assert_eq!(ship[0], ship[1], "ship tier");
    assert_eq!(interpreter[0], interpreter[1], "interpreter");
    let [dev, ship, interpreter] = live_at_two_counts(ALL_FORM, "all held\n");
    assert!(dev[1] > dev[0], "dev JIT control: {dev:?}");
    assert!(ship[1] > ship[0], "ship tier control: {ship:?}");
    assert!(
        interpreter[1] > interpreter[0],
        "interpreter control: {interpreter:?}"
    );
}

/// Rule 10 under collection mode 2 (each poll collects): the object of
/// the winner is intact when read after churn, and the inputs that lost
/// are intact when awaited later. Cost: one ship-tier C build, one dev JIT
/// compile, and one interpreter run.
#[test]
fn race_results_survive_each_poll_collection_in_the_three_tiers() {
    const CHURN: &str = r#"
class Box { values: i32[]; constructor(n: i32) { this.values = [n, n + 1, n + 2]; } }
async function nothing(): Promise<void> { return; }
async function boxed(n: i32, turns: i32): Promise<Box> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
  return new Box(n);
}
function churn(): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < 200; i += 1) { const b: Box = new Box(i); s += b.values[2]; }
  return s;
}
export async function main(): Promise<void> {
  const slow: Promise<Box> = boxed(10, 4);
  const late: Promise<Box> = boxed(20, 6);
  const won: Box = await Promise.race([slow, boxed(30, 1), late]);
  const s: i32 = churn();
  await nothing();
  print(`won ${won.values[0]} ${won.values[2]} ${s}`);
  const a: Box = await slow;
  print(`slow ${a.values[1]} ${churn()}`);
  const b: Box = await late;
  print(`late ${b.values[2]}`);
}
"#;
    let expected = "won 30 32 20300\nslow 11 20300\nlate 22\n";
    assert_eq!(dev(CHURN, CollectionMode::EachPoll).0, expected, "dev JIT");
    assert_eq!(ship(CHURN, 2).0, expected, "ship tier");
    assert_eq!(
        interpreted(CHURN, CollectionMode::EachPoll).0,
        expected,
        "interpreter"
    );
}
