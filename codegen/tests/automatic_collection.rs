//! Automatic collection at the polls of the LIR (`compiler.md` §342 rules 7,
//! 9, and 11; acceptance 2).
//!
//! The ship tier gets the collection mode from a pre-init host hook, and a
//! post-run host hook prints live bytes. The dev JIT gets the mode through
//! `RunConfig::collection`, and the interpreter through
//! `interpret_with_collection`. Cost: one ship-tier C build per run, one
//! dev JIT compile per run, and one debug interpreter run per program.

use std::path::PathBuf;

use subscript_codegen::interpreter::{interpret, interpret_with_collection};
use subscript_codegen::lir::lower_module;
use subscript_codegen::{run_c_aot_configured, run_jit_configured, NativeLibrary, RunConfig};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::context::CollectionMode;

/// The minimum headroom of the reclamation runs.
const MIN_HEADROOM: usize = 64 << 10;

/// The `tree` workload without its `free` function.
fn tree_source(depth: i32, count: i32) -> String {
    format!(
        "const DEPTH: i32 = {depth};\n\
         const COUNT: i32 = {count};\n\
         class Node {{\n\
           left: Node | null;\n\
           right: Node | null;\n\
           constructor(left: Node | null, right: Node | null) {{ this.left = left; this.right = right; }}\n\
         }}\n\
         function build(depth: i32): Node {{\n\
           if (depth === 0) {{ return new Node(null, null); }}\n\
           return new Node(build(depth - 1), build(depth - 1));\n\
         }}\n\
         function check(node: Node): i32 {{\n\
           const left: Node | null = node.left;\n\
           const right: Node | null = node.right;\n\
           if (left === null) {{ return 1; }}\n\
           if (right === null) {{ return 1; }}\n\
           return 1 + check(left) + check(right);\n\
         }}\n\
         export function main(): void {{\n\
           let checksum: i64 = 0;\n\
           for (let i: i32 = 0; i < COUNT; i += 1) {{\n\
             const root: Node = build(DEPTH);\n\
             checksum += (check(root) as i64);\n\
           }}\n\
           print(`${{checksum}}`);\n\
         }}\n"
    )
}

/// Aliases, array elements, a closure environment, and a value across a
/// call, each live at a poll.
const ROOTS: &str = r#"
class Box { v: i32; constructor(v: i32) { this.v = v; } }
function make(v: i32): Box { return new Box(v); }
function keep(b: Box, n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const t: Box = make(i); s += t.v + b.v; }
  return s;
}
export function main(): void {
  const items: Box[] = [];
  for (let i: i32 = 0; i < 50; i += 1) { items.push(new Box(i)); }
  const alias: Box = items[7];
  const offset: i32 = 3;
  const f: (x: i32) => Box = (x: i32): Box => new Box(x + offset + alias.v);
  let total: i32 = 0;
  for (let i: i32 = 0; i < 20; i += 1) {
    const b: Box = f(i);
    total += keep(b, 5) + b.v;
  }
  let text: string = "";
  for (let i: i32 = 0; i < 10; i += 1) { text = text + `${items[i].v},`; }
  print(`${total} ${alias.v} ${items.length} ${text}`);
}
"#;

/// Runs the ship tier with the mode set before the initializer. Returns the
/// program output, live bytes after the run, and the automatic collection
/// count that the post-run hook reads (`compiler.md` §345 rule 4).
fn ship(source: &str, mode: i32) -> (String, u64, u64) {
    let run = ship_counted(source, mode);
    (run.output, run.live_bytes, run.collections)
}

/// The output and the counts of one run in one tier.
#[derive(Debug)]
struct CountedRun {
    output: String,
    live_bytes: u64,
    collections: u64,
    outermost_exits: u64,
}

/// [`ship`], and also the outermost exit count that the post-run hook reads.
fn ship_counted(source: &str, mode: i32) -> CountedRun {
    let scratch = tempfile_dir();
    let hooks = scratch.join("collection_hooks.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdint.h>\n\
             #include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             uint64_t subscript_rt_automatic_collection_count(const subscript_rt_context* ctx);\n\
             uint64_t subscript_rt_outermost_exit_count(const subscript_rt_context* ctx);\n\
             void subscript_test_collection_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, {mode}, {MIN_HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             void subscript_test_collection_report(subscript_rt_context* ctx) {{\n\
               printf(\"live-bytes=%llu\\n\", (unsigned long long)subscript_rt_ctx_live_bytes(ctx));\n\
               printf(\"automatic-collections=%llu\\n\", (unsigned long long)subscript_rt_automatic_collection_count(ctx));\n\
               printf(\"outermost-exits=%llu\\n\", (unsigned long long)subscript_rt_outermost_exit_count(ctx));\n\
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
    config.pre_init_hook = Some("subscript_test_collection_setup");
    config.post_run_hook = Some("subscript_test_collection_report");
    let files = [SourceFile::new("collection.ts", source)];
    let output = run_c_aot_configured(&files, config).expect("ship run");
    let _ = std::fs::remove_dir_all(&scratch);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    let mut live = None;
    let mut collections = None;
    let mut exits = None;
    let mut program = String::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("live-bytes=") {
            live = Some(value.parse::<u64>().expect("live bytes"));
        } else if let Some(value) = line.strip_prefix("automatic-collections=") {
            collections = Some(value.parse::<u64>().expect("automatic collections"));
        } else if let Some(value) = line.strip_prefix("outermost-exits=") {
            exits = Some(value.parse::<u64>().expect("outermost exits"));
        } else {
            program.push_str(line);
            program.push('\n');
        }
    }
    CountedRun {
        output: program,
        live_bytes: live.expect("the post-run hook printed live bytes"),
        collections: collections.expect("the post-run hook printed the collection count"),
        outermost_exits: exits.expect("the post-run hook printed the exit count"),
    }
}

fn tempfile_dir() -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "subscript-automatic-collection-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).expect("create scratch directory");
    directory
}

/// Runs the dev JIT with the mode set before the initializer. Returns the
/// program output, the automatic collection count, and live bytes after
/// the run.
fn dev(source: &str, mode: CollectionMode) -> (String, u64, u64) {
    let files = [SourceFile::new("collection.ts", source)];
    let mut config = RunConfig::default();
    config.memory_accounting = true;
    config.collection = Some((mode, MIN_HEADROOM));
    let output = run_jit_configured(&files, config).expect("dev run");
    let accounting = output.memory_accounting.expect("dev accounting");
    (
        String::from_utf8(output.stdout).expect("utf-8 output"),
        accounting.automatic_collections,
        accounting.live_bytes,
    )
}

fn interpreted(source: &str, mode: CollectionMode) -> (String, u64, usize) {
    let files = [SourceFile::new("collection.ts", source)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (output, run) =
        interpret_with_collection(&module, mode, MIN_HEADROOM).expect("interpreter run");
    (
        String::from_utf8(output).expect("utf-8 output"),
        run.automatic_collections,
        run.live_bytes,
    )
}

/// Acceptance 2 in the ship tier, at full size. Without a collection, live
/// bytes after the run are the bytes of 30 trees. With mode 1, the threshold
/// after a collection is at most twice the live set, which is at most one
/// tree, so live bytes stay under three trees. Measured: about 3 s, most of
/// it the three C builds.
#[test]
fn the_tree_workload_reclaims_in_one_host_call_in_the_ship_tier() {
    let (output, one_tree, _) = ship(&tree_source(16, 1), 0);
    assert_eq!(output, "131071\n");
    let (output, off, _) = ship(&tree_source(16, 30), 0);
    assert_eq!(output, "3932130\n");
    assert!(off >= 29 * one_tree, "off {off}, one tree {one_tree}");
    let (output, live, _) = ship(&tree_source(16, 30), 1);
    assert_eq!(output, "3932130\n");
    assert!(live < 3 * one_tree, "live {live}, one tree {one_tree}");
}

/// The `tree` workload as the export `trees`, with an empty `main`.
fn tree_export_source(depth: i32, count: i32) -> String {
    tree_source(depth, count).replace(
        "export function main(): void {",
        "export function trees(): void {",
    ) + "export function main(): void {}\n"
}

/// Runs [`tree_export_source`] in the ship tier. A pre-init hook sets the
/// mode, and a pre-entry hook calls `trees`, then prints live bytes. The
/// call has no `subscript_rt_ctx_enter_script` and
/// `subscript_rt_ctx_exit_script` bracket, so no outermost exit runs between
/// the call and the print (`compiler.md` §342 rule 8a): only a poll can
/// collect. Returns the program output and live bytes after the call.
fn ship_unbracketed(depth: i32, count: i32, mode: i32) -> (String, u64) {
    let scratch = tempfile_dir();
    let hooks = scratch.join("unbracketed_hooks.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdio.h>\n\
             #include \"program.h\"\n\
             void subscript_test_unbracketed_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, {mode}, {MIN_HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             void subscript_test_unbracketed_call(subscript_rt_context* ctx) {{\n\
               subscript_export_trees(ctx);\n\
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
    config.pre_init_hook = Some("subscript_test_unbracketed_setup");
    config.pre_entry_hook = Some("subscript_test_unbracketed_call");
    let source = tree_export_source(depth, count);
    let files = [SourceFile::new("collection.ts", source.as_str())];
    let output = run_c_aot_configured(&files, config).expect("ship run");
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
    (
        program,
        live.expect("the pre-entry hook printed live bytes"),
    )
}

/// Acceptance 2 in the ship tier, where only a poll can reclaim: the hook
/// reads live bytes after an unbracketed call, before any outermost exit.
/// Mode 0 keeps at least 29 trees. Mode 1 with the small headroom keeps
/// under 3, so the polls of the ship tier collected. Cost: three ship-tier
/// C builds of about 1 s each.
#[test]
fn the_tree_workload_reclaims_at_the_polls_of_the_ship_tier() {
    let (output, one_tree) = ship_unbracketed(16, 1, 0);
    assert_eq!(output, "131071\n");
    let (output, off) = ship_unbracketed(16, 30, 0);
    assert_eq!(output, "3932130\n");
    assert!(off >= 29 * one_tree, "off {off}, one tree {one_tree}");
    let (output, live) = ship_unbracketed(16, 30, 1);
    assert_eq!(output, "3932130\n");
    assert!(live < 3 * one_tree, "live {live}, one tree {one_tree}");
}

/// Acceptance 2 in the dev JIT, at full size. The bound argument is the
/// same as in the ship tier. The mode 1 run executes the cold path of the
/// JIT poll. Measured: about 8 s in a debug build.
#[test]
fn the_tree_workload_reclaims_in_one_host_call_in_the_dev_jit() {
    let (output, collections, one_tree) = dev(&tree_source(16, 1), CollectionMode::Off);
    assert_eq!((output.as_str(), collections), ("131071\n", 0));
    let (output, collections, off) = dev(&tree_source(16, 30), CollectionMode::Off);
    assert_eq!((output.as_str(), collections), ("3932130\n", 0));
    assert!(off >= 29 * one_tree, "off {off}, one tree {one_tree}");
    let (output, collections, live) = dev(&tree_source(16, 30), CollectionMode::Automatic);
    assert_eq!(output, "3932130\n");
    assert!(collections > 1, "{collections} automatic collections");
    assert!(live < 3 * one_tree, "live {live}, one tree {one_tree}");
}

/// Acceptance 2 in the debug interpreter, with 8 trees of depth 11, because
/// a full-size run takes minutes there. The bound argument is the same as in
/// the ship tier: without a collection live bytes are 8 trees, and with
/// mode 1 they stay under 3. Measured: about 2.5 s.
#[test]
fn the_tree_workload_reclaims_in_one_host_call_in_the_interpreter() {
    let (output, collections, one_tree) = interpreted(&tree_source(11, 1), CollectionMode::Off);
    assert_eq!((output.as_str(), collections), ("4095\n", 0));
    let (output, _, off) = interpreted(&tree_source(11, 8), CollectionMode::Off);
    assert_eq!(output, "32760\n");
    assert!(off >= 7 * one_tree, "off {off}, one tree {one_tree}");
    let (output, collections, live) = interpreted(&tree_source(11, 8), CollectionMode::Automatic);
    assert_eq!(output, "32760\n");
    assert!(collections > 1, "{collections} automatic collections");
    assert!(live < 3 * one_tree, "live {live}, one tree {one_tree}");
}

/// Mode 2 collects at each poll. Each tier gives the output of a run with
/// no collection.
#[test]
fn each_poll_collection_keeps_every_live_value() {
    let files = [SourceFile::new("collection.ts", ROOTS)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let expected = String::from_utf8(interpret(&module).expect("reference run")).expect("utf-8");
    let (output, collections, _) = interpreted(ROOTS, CollectionMode::EachPoll);
    assert_eq!(output, expected);
    assert!(collections > 1, "{collections} automatic collections");
    assert_eq!(ship(ROOTS, 2).0, expected);
    let (output, collections, _) = dev(ROOTS, CollectionMode::EachPoll);
    assert_eq!(output, expected);
    assert!(collections > 1, "{collections} automatic collections");
}

/// Static-callback `map` and `filter` whose callbacks change the length of
/// the receiver. The loop bound is the length when the loop starts, so the
/// result never exceeds its reserved capacity (`compiler.md` §342 rule
/// 10b). With the live length as the bound, the `a` and `c` loops push
/// more elements than the capacity, and the interpreter check of rule 10a
/// reports an `Internal` error.
const RESERVED: &str = r#"
export function main(): void {
  const a: i32[] = [1, 2, 3];
  const m: i32[] = a.map((x: i32): i32 => {
    if (a.length < 40) { a.push(x * 10); }
    return x + 1;
  });
  const f: i32[] = a.filter((x: i32): boolean => {
    if (a.length < 40) { a.push(x); }
    return x < 10;
  });
  const c: i32[] = [7, 8, 9];
  const g: i32[] = c.filter((x: i32): boolean => {
    c.length = 0;
    for (let k: i32 = 0; k < 20; k += 1) { c.push(k); }
    return true;
  });
  const b: i32[] = [4, 5, 6];
  const n: i32[] = b.map((x: i32): i32 => { b.length = 0; return x; });
  print(`${m.join(",")} ${f.join(",")} ${a.length} ${g.join(",")} ${c.length} ${n.join(",")}`);
}
"#;

/// The reserved push of each static callback stays in its capacity when
/// the callback changes the receiver, in the three tiers.
#[test]
fn a_receiver_change_keeps_the_push_in_the_reserved_capacity() {
    let expected = "2,3,4 1,2,3 12 7,1,2 20 4\n";
    let files = [SourceFile::new("collection.ts", RESERVED)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let output = interpret(&module).expect("interpreter run");
    assert_eq!(String::from_utf8(output).expect("utf-8"), expected);
    assert_eq!(dev(RESERVED, CollectionMode::Off).0, expected);
    assert_eq!(ship(RESERVED, 0).0, expected);
}

fn corpus_source(path: &str) -> String {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::fs::read_to_string(repo.join("corpus").join(path)).expect("read corpus entry")
}

/// The outermost exit count and the output of one run in the interpreter
/// and in the dev JIT.
fn exit_counts(source: &str, mode: CollectionMode) -> ((String, u64), (String, u64)) {
    let files = [SourceFile::new("collection.ts", source)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (output, run) =
        interpret_with_collection(&module, mode, MIN_HEADROOM).expect("interpreter run");
    let mut config = RunConfig::default();
    config.memory_accounting = true;
    config.collection = Some((mode, MIN_HEADROOM));
    let dev = run_jit_configured(&files, config).expect("dev run");
    let accounting = dev.memory_accounting.expect("dev accounting");
    (
        (
            String::from_utf8(output).expect("utf-8 output"),
            run.outermost_exits,
        ),
        (
            String::from_utf8(dev.stdout).expect("utf-8 output"),
            accounting.outermost_exits,
        ),
    )
}

/// `compiler.md` §342 rule 16: the interpreter and the dev JIT mark the same
/// count of outermost exits. The shapes: a plain `main`, an async `main`
/// with suspensions, a generator, a sync `main` with two async exports, a
/// failed task with an unreachable holder (`t116`, with mode 0, because a
/// trapped dev JIT run gives no accounting), and held failed handles. The
/// ship tier gives no host function that reads the count, so the test
/// compares two tiers. Cost: six programs, each one dev JIT compile and one
/// debug interpreter run per mode.
#[test]
fn the_outermost_exit_count_is_equal_in_the_interpreter_and_the_dev_jit() {
    let entries = [
        ("accept/a01-hello.ts", true),
        ("accept/a407-fixed-array-traversal-suspension.ts", true),
        ("accept/a20-coroutine-generator.ts", true),
        ("accept/a94-async-two-roots.ts", true),
        ("trap/t116-detected-task-failure.ts", false),
        ("accept/a258-await-delivers-exception.ts", true),
    ];
    let mut counts = Vec::new();
    for (path, automatic) in entries {
        let source = corpus_source(path);
        let modes: &[CollectionMode] = if automatic {
            &[CollectionMode::Off, CollectionMode::Automatic]
        } else {
            &[CollectionMode::Off]
        };
        for &mode in modes {
            let (interpreter, dev) = exit_counts(&source, mode);
            assert_eq!(interpreter, dev, "{path} {mode:?}");
            counts.push(interpreter.1);
        }
    }
    // An async run has more exits than a plain run: init, main, the kick,
    // and one for each checkpoint that dispatches.
    assert_eq!(counts[0], 3, "a plain main has three exits");
    assert!(counts[2] > 4, "{counts:?}");
}

/// A program that holds one failed handle for 2,000 steps, then observes it
/// (`compiler.md` §342 problem 5 and acceptance 5).
const HELD: &str = r#"
async function fail(): Promise<void> { throw new Error("held"); }
export async function main(): Promise<void> {
  const h: Promise<void> = fail();
  for (let i: i32 = 0; i < 2000; i += 1) { await Context.suspend(); }
  try { await h; } catch (e) { print("caught"); }
}
"#;

/// Acceptance 5: the held handle runs at most 11 detection collections in
/// 2,000 exits, and prints `caught`. The interpreter and the dev JIT read
/// the count; the ship tier gives no host function that reads it, so it
/// states the output. Cost: one debug interpreter run of 2,000 steps, one
/// dev JIT compile, and one ship-tier C build.
#[test]
fn a_held_failed_handle_backs_off_and_is_caught() {
    let files = [SourceFile::new("collection.ts", HELD)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (output, run) =
        interpret_with_collection(&module, CollectionMode::Automatic, 0).expect("interpreter run");
    assert_eq!(String::from_utf8(output).expect("utf-8"), "caught\n");
    assert!(run.outermost_exits >= 2000, "{run:?}");
    assert!((1..=11).contains(&run.detection_collections), "{run:?}");
    let mut config = RunConfig::default();
    config.memory_accounting = true;
    config.collection = Some((CollectionMode::Automatic, 0));
    let dev = run_jit_configured(&files, config).expect("dev run");
    let accounting = dev.memory_accounting.expect("dev accounting");
    assert_eq!(dev.stdout, b"caught\n");
    assert_eq!(accounting.outermost_exits, run.outermost_exits);
    assert_eq!(accounting.detection_collections, run.detection_collections);
    assert_eq!(ship(HELD, 1).0, "caught\n");
}

/// A failed task whose holder ends in `hold`, then a loop with a backedge
/// poll in `work`. `COLLECT` is the place of an explicit collection.
const POLL_RECORDS: &str = r#"
class Box { value: Promise<void>; constructor(value: Promise<void>) { this.value = value; } }
class Cell { v: i32; constructor(v: i32) { this.v = v; } }
async function fail(message: string): Promise<void> { throw new Error(message); }
function hold(message: string): void {
  const box = new Box(fail(message));
}
function work(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const c: Cell = new Cell(i); s += c.v; }
  return s;
}
export function main(): void {
  hold("lost");
  print(`${work(4)}`);
  COLLECT
  print("after");
}
"#;

/// The trap of one run: output before the trap, message, line, and column.
fn interpreter_trap(source: &str, mode: CollectionMode) -> (String, String, u32, u32) {
    use subscript_codegen::interpreter::InterpretError;
    let files = [SourceFile::new("collection.ts", source)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let error = interpret_with_collection(&module, mode, MIN_HEADROOM).expect_err("trap");
    let output = String::from_utf8(error.output().to_vec()).expect("utf-8");
    let InterpretError::Execution { source, .. } = error else {
        panic!("the run must keep its output");
    };
    let InterpretError::Trap {
        kind, pos, message, ..
    } = *source
    else {
        panic!("the run must trap: {source}");
    };
    assert_eq!(kind, "uncaught-exception");
    (output, message, pos.line, pos.col)
}

fn dev_trap(source: &str, mode: CollectionMode) -> (String, String, u32, u32) {
    let files = [SourceFile::new("collection.ts", source)];
    let mut config = RunConfig::default();
    // The accounting run executes in this process; a run without it forks,
    // and a fork needs a process with one thread (compiler.md §190.1).
    config.memory_accounting = true;
    config.collection = Some((mode, MIN_HEADROOM));
    match run_jit_configured(&files, config) {
        Err(subscript_codegen::RunError::Trap(report)) => (
            String::from_utf8(report.stdout).expect("utf-8"),
            report.message,
            report.pos.line,
            report.pos.col,
        ),
        other => panic!("the dev run must trap: {other:?}"),
    }
}

/// Rule 12: in mode 2 the poll of `work` releases the holder and records
/// the failure, so `after` prints. The next outermost exit runs the
/// detection collection, which reports at the `throw` (4:55). Control: in
/// mode 0 no collection runs, and the run prints `after` with no trap.
#[test]
fn a_poll_records_a_failure_and_the_detection_reports_it_at_the_throw() {
    let source = POLL_RECORDS.replace("COLLECT", "");
    let expected = ("6\nafter\n".to_string(), "Error: lost".to_string(), 4, 55);
    assert_eq!(
        interpreter_trap(&source, CollectionMode::EachPoll),
        expected
    );
    assert_eq!(dev_trap(&source, CollectionMode::EachPoll), expected);
    let files = [SourceFile::new("collection.ts", &source)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (output, _) =
        interpret_with_collection(&module, CollectionMode::Off, 0).expect("no collection, no trap");
    assert_eq!(output, b"6\nafter\n");
}

/// Rule 13: the explicit collection reports a failure that a poll recorded
/// (mode 2) at its own position (16:3), as it reports a failure that it
/// finds itself (mode 0).
#[test]
fn an_explicit_collection_reports_a_recorded_failure_at_its_call() {
    let source = POLL_RECORDS.replace("COLLECT", "Context.collect();");
    let expected = ("6\n".to_string(), "Error: lost".to_string(), 16, 3);
    for mode in [CollectionMode::EachPoll, CollectionMode::Off] {
        assert_eq!(interpreter_trap(&source, mode), expected, "{mode:?}");
        assert_eq!(dev_trap(&source, mode), expected, "{mode:?}");
    }
}

/// An entry that allocates in straight-line code: no loop and no call, so
/// no poll. Only the outermost exit serves the flag (`compiler.md` §342
/// rule 8).
const STEP: &str = r#"
class Cell { a: i32; b: i32; c: i32; d: i32; constructor(a: i32) { this.a = a; this.b = a; this.c = a; this.d = a; } }
export function step(): void {
  const first: Cell = new Cell(1);
  const second: Cell = new Cell(2);
  const items: i32[] = [first.a, second.b, 3, 4, 5, 6, 7, 8];
  const text: string = "ab".repeat(2048) + `${items.length}`;
}
export function main(): void {}
"#;

/// The calls that the host makes in the acceptance 3 runs.
const HOST_CALLS: u64 = 10_000;

/// Runs [`STEP`] in the ship tier: a C host hook sets the mode before the
/// initializer, then calls `step` [`HOST_CALLS`] times. The host brackets
/// each call with `subscript_rt_ctx_enter_script` and
/// `subscript_rt_ctx_exit_script`; the exit is the outermost exit of rule 8. Returns live bytes
/// after an explicit collection before the first call, the bytes of the
/// first call, and live bytes after the last call.
fn ship_host_calls(mode: i32) -> (u64, u64, u64) {
    let scratch = tempfile_dir();
    let hooks = scratch.join("host_calls.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdio.h>\n\
             #include \"program.h\"\n\
             void subscript_test_host_calls_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, {mode}, {MIN_HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             void subscript_test_host_calls(subscript_rt_context* ctx) {{\n\
               subscript_rt_ctx_collect(ctx);\n\
               unsigned long long base = subscript_rt_ctx_live_bytes(ctx);\n\
               subscript_rt_ctx_enter_script(ctx);\n\
               subscript_export_step(ctx);\n\
               subscript_rt_ctx_exit_script(ctx);\n\
               unsigned long long one = subscript_rt_ctx_live_bytes(ctx) - base;\n\
               for (int i = 1; i < {HOST_CALLS}; i += 1) {{\n\
                 subscript_rt_ctx_enter_script(ctx);\n\
                 subscript_export_step(ctx);\n\
                 subscript_rt_ctx_exit_script(ctx);\n\
               }}\n\
               printf(\"%llu %llu %llu\\n\", base, one, (unsigned long long)subscript_rt_ctx_live_bytes(ctx));\n\
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
    config.pre_init_hook = Some("subscript_test_host_calls_setup");
    config.pre_entry_hook = Some("subscript_test_host_calls");
    let files = [SourceFile::new("collection.ts", STEP)];
    let output = run_c_aot_configured(&files, config).expect("ship run");
    let _ = std::fs::remove_dir_all(&scratch);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    let values: Vec<u64> = text
        .split_whitespace()
        .map(|value| value.parse().expect("a count"))
        .collect();
    let [base, one, last] = values[..] else {
        panic!("the hook prints three counts: {text:?}");
    };
    (base, one, last)
}

/// Acceptance 3 in the ship tier. With mode 1 the threshold after a
/// collection is the live set plus the larger of the live set and the
/// minimum headroom (rule 6), so live bytes after the last call stay under
/// that threshold plus one call. Control: with mode 0 the bytes of each call
/// stay. Measured: 83,530,016 live bytes with mode 0, and 16 with mode 1
/// (threshold 65,536, one call 8,369). Cost: two ship-tier C builds of
/// about 0.5 s each.
#[test]
fn host_calls_with_no_poll_reclaim_at_the_outermost_exit_in_the_ship_tier() {
    let (base, one, off) = ship_host_calls(0);
    assert!(one > 0, "a call allocates");
    // The first call is 16 bytes larger than the next calls (8,369 bytes,
    // and 83,530,016 bytes after 10,000 calls), so the bound keeps nine
    // tenths of the calls.
    assert!(
        off >= base + HOST_CALLS * one * 9 / 10,
        "off {off}, base {base}, one call {one}"
    );
    let (base, one, live) = ship_host_calls(1);
    let threshold = base + base.max(MIN_HEADROOM as u64);
    println!("mode 0: {off} bytes; mode 1: {live} bytes, threshold {threshold}, one call {one}");
    assert!(
        live <= threshold + one,
        "live {live}, threshold {threshold}, one call {one}"
    );
}

/// Runs [`STEP`] in a reload session with `mode` and the default minimum
/// headroom: one call, then the other calls of [`HOST_CALLS`]. Each call of
/// the session is an outermost exit (rule 8a). Returns live allocations
/// before the first call, the allocations of the first call, and live
/// allocations after the last call.
fn dev_host_calls(mode: CollectionMode) -> (usize, usize, usize) {
    let files = [SourceFile::new("collection.ts", STEP)];
    let mut config = RunConfig::default();
    config.collection = Some((mode, 0));
    let mut session =
        subscript_codegen::ReloadSession::new_configured(&files, config).expect("session");
    let base = session.live_allocations();
    session.call_export("step").expect("first call");
    let one = session.live_allocations() - base;
    for _ in 1..HOST_CALLS {
        session.call_export("step").expect("call");
    }
    (base, one, session.live_allocations())
}

/// Acceptance 3 in the dev JIT. A reload session calls the export as a host
/// does, with mode 1 and the default minimum headroom of 16 MiB, and reads
/// live allocations. A call makes 8 allocations of about 8 KiB in all, so
/// 10,000 calls allocate more than twice the headroom. With rule 8 at most
/// the allocations of the calls in one headroom stay, which is under half
/// of them. Control: with mode 0 the allocations of each call stay.
/// Measured: 13,525 of 70,001 allocations stay with mode 1, and 70,001
/// with mode 0. Cost: two dev JIT compiles and 20,000 calls, under 1 s
/// (release).
#[test]
fn host_calls_with_no_poll_reclaim_at_the_outermost_exit_in_the_dev_jit() {
    let (base, one, off) = dev_host_calls(CollectionMode::Off);
    assert!(one > 0, "a call allocates");
    // The first call makes 8 allocations and each later call 7, so the
    // bound is seven eighths of the first call for each call.
    assert!(
        off >= base + one * HOST_CALLS as usize * 7 / 8,
        "off {off}, base {base}, one call {one}"
    );
    let (base, one, live) = dev_host_calls(CollectionMode::Automatic);
    println!("live allocations: mode 0 {off}; mode 1 base {base}, one call {one}, after the last call {live}");
    assert!(
        live < base + one * HOST_CALLS as usize / 2,
        "live {live}, base {base}, one call {one}"
    );
}

/// A reload session applies its collection mode before the initializer, and
/// a reload keeps it, because the Context lives across the reload
/// (`compiler.md` §342 rule 17). With mode 2 each outermost exit collects,
/// so after the reload ten calls leave at most the allocations of one call.
/// Control: a session with the default mode 1 keeps the allocations of the
/// ten calls, because they are under the 16 MiB headroom. Cost: two
/// sessions, one reload, and 20 calls.
#[test]
fn a_reload_session_keeps_its_collection_mode_across_a_reload() {
    let files = [SourceFile::new("collection.ts", STEP)];
    let calls_after_reload = |collection: Option<(CollectionMode, usize)>| {
        let mut config = RunConfig::default();
        config.collection = collection;
        let mut session =
            subscript_codegen::ReloadSession::new_configured(&files, config).expect("session");
        session.reload(&files).expect("reload");
        let base = session.live_allocations();
        session.call_export("step").expect("first call");
        let one = session.live_allocations().saturating_sub(base);
        for _ in 1..10 {
            session.call_export("step").expect("call");
        }
        (base, one, session.live_allocations())
    };
    let (base, one, kept) = calls_after_reload(None);
    assert!(one > 0, "a call allocates");
    assert!(
        kept >= base + one * 10 * 7 / 8,
        "kept {kept}, base {base}, one call {one}"
    );
    let (base, _, live) = calls_after_reload(Some((CollectionMode::EachPoll, 0)));
    assert!(
        live <= base + one,
        "live {live}, base {base}, one call {one}"
    );
}

/// An entry that allocates a string of 512 KiB, and an initializer that
/// traps when `INDEX` is out of bounds.
const LARGE_STEP: &str = r#"
const xs: i32[] = [1, 2];
const first: i32 = xs[INDEX];
export function step(): void {
  const text: string = "ab".repeat(262144);
}
export function main(): void {}
"#;

/// The calls after the reload: 48 strings of 512 KiB are 24 MiB, over the
/// default minimum headroom of 16 MiB.
const LARGE_CALLS: usize = 48;

/// A reload after an initializer trap builds a new Context with the
/// configured mode and minimum headroom (`compiler.md` §342 rule 17). The
/// session has no function that reads the mode, so the test shows it by
/// behaviour: mode 0, and mode 1 with a minimum headroom of 64 MiB, keep
/// each string of the calls after the reload. Control: the default mode 1
/// with 16 MiB collects, and keeps under three quarters of them. Cost:
/// three sessions, three reloads, and 144 calls.
#[test]
fn a_reload_after_an_initializer_trap_keeps_the_collection_mode() {
    let trapping = [SourceFile::new(
        "collection.ts",
        LARGE_STEP.replace("INDEX", "5"),
    )];
    let corrected = [SourceFile::new(
        "collection.ts",
        LARGE_STEP.replace("INDEX", "1"),
    )];
    let calls_after_reload = |collection: Option<(CollectionMode, usize)>| {
        let mut config = RunConfig::default();
        config.collection = collection;
        let (mut session, trap) =
            subscript_codegen::ReloadSession::new_capturing_initializer_trap_configured(
                &trapping, config,
            )
            .expect("session");
        assert!(trap.is_some(), "the initializer traps");
        session.reload(&corrected).expect("reload");
        let base = session.live_allocations();
        session.call_export("step").expect("first call");
        let one = session.live_allocations() - base;
        for _ in 1..LARGE_CALLS {
            session.call_export("step").expect("call");
        }
        (base, one, session.live_allocations())
    };
    for collection in [
        (CollectionMode::Off, 0),
        (CollectionMode::Automatic, 64 << 20),
    ] {
        let (base, one, live) = calls_after_reload(Some(collection));
        assert!(one > 0, "a call allocates");
        // Each call leaves its string.
        assert!(
            live >= base + LARGE_CALLS,
            "{collection:?}: live {live}, base {base}, one call {one}"
        );
    }
    let (base, one, live) = calls_after_reload(None);
    assert!(
        live < base + LARGE_CALLS * 3 / 4,
        "default: live {live}, base {base}, one call {one}"
    );
}

/// compiler.md §344 acceptance 1a: an array of reference objects that is
/// the only holder of its elements keeps each element through a forced
/// collection before a growth of its buffer and after it. The same shape
/// with an `f64[]` and an `i32[]` reads the same values.
const ARRAY_HOLDER: &str = r#"
class Box { v: i32; constructor(v: i32) { this.v = v; } }
export function main(): void {
  const boxes: Box[] = [];
  const floats: f64[] = [];
  const ints: i32[] = [];
  for (let i: i32 = 0; i < 100; i += 1) {
    boxes.push(new Box(i * 7 + 3));
    floats.push((i * 7 + 3) as f64);
    ints.push(i * 7 + 3);
  }
  Context.collect();
  for (let i: i32 = 100; i < 150; i += 1) {
    boxes.push(new Box(i * 7 + 3));
    floats.push((i * 7 + 3) as f64);
    ints.push(i * 7 + 3);
  }
  Context.collect();
  let boxSum: i32 = 0;
  let floatSum: f64 = 0.0;
  let intSum: i32 = 0;
  for (let i: i32 = 0; i < boxes.length; i += 1) {
    boxSum += boxes[i].v;
    floatSum += floats[i];
    intSum += ints[i];
  }
  print(`${boxes.length} ${boxSum} ${floatSum} ${intSum}`);
}
"#;

/// Acceptance 1a in the three tiers, in the default mode and in mode 2
/// (each poll collects). The sum of `7 i + 3` for `i` in `0..150` is
/// `7 * (149 * 150 / 2) + 3 * 150 = 78675`. Cost: two ship-tier C builds,
/// two dev JIT compiles, and two interpreter runs.
#[test]
fn an_array_of_references_keeps_its_elements_in_the_three_tiers() {
    let expected = format!("150 {} {} {}\n", 78675, 78675, 78675);
    for mode in [CollectionMode::Automatic, CollectionMode::EachPoll] {
        let (output, _, _) = ship(ARRAY_HOLDER, mode as i32);
        assert_eq!(output, expected, "ship tier, {mode:?}");
        let (output, _, _) = dev(ARRAY_HOLDER, mode);
        assert_eq!(output, expected, "dev JIT, {mode:?}");
        let (output, _, _) = interpreted(ARRAY_HOLDER, mode);
        assert_eq!(output, expected, "interpreter, {mode:?}");
    }
}

/// The arrays that `Promise.all` makes, for the handle-fact tests.
const PROMISE_ALL: &str = r#"
class Box { v: i32; constructor(v: i32) { this.v = v; } }
async function number(x: i32): Promise<i32> { return x; }
async function boxed(x: i32): Promise<Box> { return new Box(x); }
let numbers: i32[] = [];
let boxes: Box[] = [];
export async function main(): Promise<void> {
  numbers = await Promise.all([number(1), number(2), number(3), number(4), number(5)]);
  boxes = await Promise.all([boxed(6), boxed(7), boxed(8), boxed(9), boxed(10), boxed(11)]);
  Context.collect();
  print(`${numbers[4]} ${boxes[5].v}`);
}
"#;

/// compiler.md §344 rule 4 in the dev JIT: an array that JIT code creates
/// has the fact of its element type. A literal, a pre-sized `map` result,
/// a function-value `map` to a number and to a reference, and the results
/// of `Promise.all`. Each kept array has its own length. Cost: two dev JIT
/// runs.
#[test]
fn arrays_that_jit_code_creates_have_the_fact_of_their_element_type() {
    const ARRAYS: &str = r#"
class Box { v: i32; constructor(v: i32) { this.v = v; } }
let kept: f64[][] = [];
let keptBoxes: Box[][] = [];
function build(): void {
  const half: (x: i32) => f64 = (x: i32): f64 => (x as f64) * 0.5;
  const wrap: (x: i32) => Box = (x: i32): Box => new Box(x);
  const xs: i32[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
  kept.push([1.5, 2.5, 3.5]);
  keptBoxes.push([new Box(1), new Box(2), new Box(3), new Box(4)]);
  kept.push(xs.slice(0, 5).map((x: i32): f64 => (x as f64) * 2.0));
  keptBoxes.push(xs.slice(0, 6).map((x: i32): Box => new Box(x)));
  kept.push(xs.slice(0, 7).map(half));
  keptBoxes.push(xs.slice(0, 8).map(wrap));
}
export function main(): void {
  build();
  Context.collect();
  print(`${kept.length} ${keptBoxes.length}`);
}
"#;
    let run = |source: &str| {
        let files = [SourceFile::new("facts.ts", source)];
        let mut config = RunConfig::default();
        config.memory_accounting = true;
        let output = run_jit_configured(&files, config).expect("dev run");
        let mut facts = output
            .memory_accounting
            .expect("accounting")
            .live_array_facts;
        facts.sort_unstable();
        (
            String::from_utf8(output.stdout).expect("utf-8 output"),
            facts,
        )
    };
    let (output, facts) = run(ARRAYS);
    assert_eq!(output, "3 3\n");
    // The two arrays of arrays have three elements each and can hold a handle.
    assert_eq!(
        facts,
        vec![
            (3, false),
            (3, false),
            (3, true),
            (4, false),
            (5, true),
            (6, false),
            (7, true),
            (8, false),
        ]
    );
    let (output, facts) = run(PROMISE_ALL);
    assert_eq!(output, "5 11\n");
    assert!(facts.contains(&(5, true)), "{facts:?}");
    assert!(facts.contains(&(6, false)), "{facts:?}");
}

/// The corpus entries of `compiler.md` §345 acceptance 1: a function-value
/// callback replaces the elements of the receiver that the runtime operation
/// holds.
const OPERATION_ROOT_ENTRIES: [&str; 4] = [
    "accept/a493-filter-keeps-the-element-that-its-callback-removes.ts",
    "accept/a494-find-keeps-the-element-that-its-callback-removes.ts",
    "accept/a495-group-by-keeps-the-element-that-its-callback-removes.ts",
    "accept/a496-sort-keeps-the-elements-that-its-comparator-replaces.ts",
];

/// One run of `source` in mode 2 (each poll and each outermost exit
/// collects) in the ship tier, the dev JIT, and the interpreter.
fn each_poll_runs(source: &str) -> [(&'static str, CountedRun); 3] {
    let ship = ship_counted(source, CollectionMode::EachPoll as i32);
    let files = [SourceFile::new("collection.ts", source)];
    let mut config = RunConfig::default();
    config.memory_accounting = true;
    config.collection = Some((CollectionMode::EachPoll, MIN_HEADROOM));
    let dev = run_jit_configured(&files, config).expect("dev run");
    let accounting = dev.memory_accounting.expect("dev accounting");
    let dev = CountedRun {
        output: String::from_utf8(dev.stdout).expect("utf-8 output"),
        live_bytes: accounting.live_bytes as u64,
        collections: accounting.automatic_collections,
        outermost_exits: accounting.outermost_exits,
    };
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (output, run) = interpret_with_collection(&module, CollectionMode::EachPoll, MIN_HEADROOM)
        .expect("interpreter run");
    let interpreter = CountedRun {
        output: String::from_utf8(output).expect("utf-8 output"),
        live_bytes: run.live_bytes as u64,
        collections: run.automatic_collections,
        outermost_exits: run.outermost_exits,
    };
    [
        ("ship tier", ship),
        ("dev JIT", dev),
        ("interpreter", interpreter),
    ]
}

/// The bound of the automatic collections that the outermost exits of a
/// run give. `compiler.md` §342 rule 8: an outermost exit runs at most one
/// automatic collection. The detection collection of the same exit has its
/// own count, so the bound is one collection per exit. A count over the
/// bound shows that a poll in the script call collected.
fn exit_collection_bound(run: &CountedRun) -> u64 {
    run.outermost_exits
}

/// `compiler.md` §345 acceptance 2: the shapes of acceptance 1 with no
/// `Context.collect()`. In mode 2 the polls collect, and each tier prints
/// the output of mode 0. Each tier gives more automatic collections than
/// its outermost exits can give, so a poll in the script call collected.
/// Control: mode 0 prints the golden of the entry, and the source has no
/// explicit collection after the edit. Cost: four programs, each two
/// ship-tier C builds, one dev JIT compile, and two debug interpreter runs.
#[test]
fn a_runtime_operation_keeps_what_it_reads_after_a_callback_with_no_explicit_collection() {
    for path in OPERATION_ROOT_ENTRIES {
        let entry = corpus_source(path);
        let source = entry.replace("  Context.collect();\n", "");
        assert_ne!(
            source, entry,
            "{path}: the entry has an explicit collection"
        );
        assert!(!source.contains("Context.collect();"), "{path}");
        let golden = corpus_source(&path.replace(".ts", ".expected"));
        let (expected, _, _) = interpreted(&source, CollectionMode::Off);
        assert_eq!(expected, golden, "{path}: mode 0");
        for (tier, run) in each_poll_runs(&source) {
            assert_eq!(run.output, expected, "{path}: {tier}");
            assert!(
                run.collections > exit_collection_bound(&run),
                "{path}: {tier}: no poll collected: {run:?}"
            );
        }
    }
}

/// The control of the bound: a program with no loop and no call-graph
/// cycle has no poll (`compiler.md` §342 rule 11), so in mode 2 only its
/// outermost exits collect. Each tier gives a count over zero and not over
/// the bound. Cost: one ship-tier C build, one dev JIT compile, and one
/// debug interpreter run.
#[test]
fn a_program_with_no_poll_collects_only_at_its_outermost_exits() {
    const NO_POLL: &str = r#"
class Box { v: i32; constructor(v: i32) { this.v = v; } }
export function main(): void {
  const b: Box = new Box(4);
  print(`${b.v}`);
}
"#;
    for (tier, run) in each_poll_runs(NO_POLL) {
        assert_eq!(run.output, "4\n", "{tier}");
        assert!(run.collections > 0, "{tier}: the exit collected: {run:?}");
        assert!(
            run.collections <= exit_collection_bound(&run),
            "{tier}: a poll collected: {run:?}"
        );
    }
}
