//! Polls on the return path of a recursion (`compiler.md` §351,
//! acceptance 1, 2, 4, and 5, and rule 12).
//!
//! Each shape runs at two depths in automatic mode with a
//! minimum headroom of 64 bytes. The sample is before the outermost exit,
//! because the exit collects (§342 rule 8).
//!
//! - **Ship tier.** A pre-entry host hook collects, samples, brackets the
//!   export `run(depth)` with `subscript_rt_ctx_enter_script` and
//!   `subscript_rt_ctx_exit_script`, and samples between the export and the
//!   exit. For an async shape, the bracket holds the kick and each step of
//!   the scheduler, so no step is an outermost exit.
//! - **Dev JIT.** A reload session calls `settle`, which collects, and
//!   samples. It then calls `probe(depth)`, which runs the shape and then
//!   stops on an index trap. A Context with a trap runs no collection at
//!   the outermost exit (§342 rule 8), so the live count after the call is
//!   the live count before the exit. For an async shape, `configure` and
//!   `start` take the place of `probe`, and the trap is in the last job of
//!   the last step. A run of `main` with memory accounting gives
//!   the automatic collection count.
//!
//! Cost: one ship-tier C build for each shape, and two dev JIT compiles for
//! each shape and depth. Each root test runs one program in the
//! interpreter two times, in the dev JIT one or two times, and in one or
//! two ship-tier builds. The workload test lowers seven programs and runs
//! no code. The async interpreter test runs two programs of depth 16 in the
//! interpreter. Wall times, debug and release: ship tier 4.5 s and 1.4 s,
//! dev JIT 1.8 s and 0.1 s, async ship tier 1.4 s and 1.2 s, async dev JIT
//! 0.6 s and 0.05 s, roots 1.0 s and 0.8 s, async roots 0.5 s and 0.4 s,
//! workloads 0.04 s and 0.01 s, async interpreter 0.02 s and under 0.01 s.

use std::path::PathBuf;

use subscript_codegen::interpreter::interpret_with_collection;
use subscript_codegen::lir::allocation::{
    instruction_allocation, InstructionAllocation, MethodBodies,
};
use subscript_codegen::lir::lower_module;
use subscript_codegen::{
    run_c_aot_configured, run_jit_configured, EntryArg, NativeLibrary, ReloadSession, RunConfig,
    RunError,
};
use subscript_compiler::{check_program, lir as l, SourceFile};
use subscript_runtime::context::CollectionMode;

#[allow(dead_code)]
mod corpus;

/// The minimum headroom of each run of acceptance 1 and 2.
const HEADROOM: usize = 64;

/// The two depths of acceptance 1.
const DEPTHS: [i32; 2] = [256, 1024];

/// The payload size of `P`: four `i64` fields in the C layout (invariant
/// 1). The trigger counts the payload bytes that allocations request
/// (§346 rule 1).
const SIZE: usize = 4 * 8;

/// The class of each shape, and the exports that the two tiers call.
/// `probe` stops on an index trap after the shape returns: the index is
/// out of range for each depth over zero.
const COMMON: &str = r#"
class P { a: i64 = 0; b: i64 = 0; c: i64 = 0; d: i64 = 0; }
export function settle(): void { Context.collect(); }
export function probe(depth: i32): void {
  run(depth);
  const slot: FixedArray<i32, 1> = [0];
  print(`${slot[depth]}`);
}
export function main(): void { run(DEPTH); }
"#;

/// A shape: its name and its source. The shapes are the fixture of §351, two functions that call each other, a
/// recursion through a function value, a cycle that a direct call closes
/// after an indirect call, a handler with a throw from the bottom, a cycle
/// that passes only through a runtime operation with a callback in a
/// variable, and the control.
struct Shape {
    name: &'static str,
    source: &'static str,
}

const SHAPES: [Shape; 7] = [
    Shape {
        name: "fixture",
        source: r#"
function build(depth: i32): P {
  if (depth === 0) { return new P(); }
  build(depth - 1);
  return new P();
}
export function run(depth: i32): void { build(depth); }
"#,
    },
    Shape {
        name: "mutual",
        source: r#"
function ping(depth: i32): P {
  if (depth === 0) { return new P(); }
  pong(depth - 1);
  return new P();
}
function pong(depth: i32): P {
  if (depth === 0) { return new P(); }
  ping(depth - 1);
  return new P();
}
export function run(depth: i32): void { ping(depth); }
"#,
    },
    Shape {
        name: "function-value",
        source: r#"
let next: (depth: i32) => P = step;
function step(depth: i32): P {
  if (depth === 0) { return new P(); }
  next(depth - 1);
  return new P();
}
export function run(depth: i32): void { next(depth); }
"#,
    },
    Shape {
        name: "mixed",
        source: r#"
const p0: P = new P();
let hook: (depth: i32) => P = g;
function f(depth: i32): P {
  if (depth === 0) { return new P(); }
  hook(depth - 1);
  return p0;
}
function g(depth: i32): P {
  if (depth === 0) { return new P(); }
  f(depth - 1);
  return new P();
}
export function run(depth: i32): void { f(depth); }
"#,
    },
    Shape {
        name: "handler",
        source: r#"
function dive(depth: i32): P {
  if (depth === 0) { throw new Error("bottom"); }
  try {
    return dive(depth - 1);
  } catch (e) {
    const p: P = new P();
    p.a = depth as i64;
    throw e;
  }
}
export function run(depth: i32): void {
  try { dive(depth); } catch (e) { }
}
"#,
    },
    Shape {
        name: "callback-operation",
        source: r#"
const one: i32[] = [0];
let level: i32 = 0;
let each: (x: i32) => void = visit;
function visit(x: i32): void {
  if (level === 0) { return; }
  level -= 1;
  one.forEach(each);
  const p: P = new P();
  p.a = x as i64;
}
export function run(depth: i32): void {
  level = depth;
  visit(0);
}
"#,
    },
    Shape {
        name: "control",
        source: r#"
const p0: P = new P();
function keep(depth: i32): P {
  if (depth === 0) { return new P(); }
  keep(depth - 1);
  return p0;
}
export function run(depth: i32): void { keep(depth); }
"#,
    },
];

/// The debug ship program on a 1 MiB main-thread stack overflows at depth
/// 1,024 for callback-operation. Use smaller depths for this shape.
fn depths(shape: &Shape) -> [i32; 2] {
    match shape.name {
        "callback-operation" => [128, 256],
        _ => DEPTHS,
    }
}

fn source(shape: &Shape, depth: i32) -> String {
    format!("const DEPTH: i32 = {depth};\n{COMMON}{}", shape.source)
}

/// The most instructions with the fact "can allocate" in one function of
/// the shape, other than calls of script functions. No function of a shape
/// has a loop, so a frame runs each of them once at most.
fn per_frame(shape: &Shape) -> usize {
    per_frame_of(&source(shape, 0))
}

/// [`per_frame`] of one program text.
fn per_frame_of(text: &str) -> usize {
    let files = [SourceFile::new("return-path.ts", text)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let methods = MethodBodies::of_module(&module);
    module
        .functions
        .iter()
        .filter(|function| !function.exported)
        .map(|function| {
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter(|instruction| {
                    instruction_allocation(&module, &methods, function, instruction)
                        == InstructionAllocation::Can
                })
                .count()
        })
        .max()
        .unwrap_or(0)
}

/// The bound of the objects over the base before the outermost exit.
///
/// After a collection the limit is the headroom while the live bytes are
/// under it (§346 rule 2). Each frame that allocates on a return path
/// allocates one `P` at least, so at most `HEADROOM / SIZE + 1` such frames
/// run before the flag is set. Each frame allocates at most `per_frame`
/// objects, and one more frame can run before its poll. The bound does not
/// depend on the depth.
fn bound(per_frame: usize) -> usize {
    (HEADROOM / SIZE + 2) * per_frame
}

/// One sample: live objects and automatic collections.
#[derive(Debug, Clone, Copy)]
struct Sample {
    objects: u64,
    collections: u64,
}

/// The objects over the base before the exit, and the automatic
/// collections in the call, of one depth in the ship tier.
#[derive(Clone, Copy)]
struct ShipFigures {
    depth: i32,
    objects: u64,
    collections: u64,
}

fn scratch(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "subscript-return-path-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).expect("create the scratch directory");
    directory
}

/// Runs each depth of a shape in the ship tier, in one build.
fn ship(shape: &Shape) -> Vec<ShipFigures> {
    let directory = scratch(shape.name);
    let hooks = directory.join("return_path_hooks.c");
    let calls = depths(shape)
        .iter()
        .map(|depth| {
            format!(
                "  subscript_rt_ctx_collect(ctx);\n  sample(ctx, {depth}, \"base\");\n\
                 \x20 subscript_rt_ctx_enter_script(ctx);\n\
                 \x20 subscript_export_run(ctx, {depth});\n\
                 \x20 sample(ctx, {depth}, \"before-exit\");\n\
                 \x20 subscript_rt_ctx_exit_script(ctx);\n"
            )
        })
        .collect::<String>();
    std::fs::write(
        &hooks,
        format!(
            "#include <stdint.h>\n\
             #include <stdio.h>\n\
             #include \"program.h\"\n\
             uint64_t subscript_rt_automatic_collection_count(const subscript_rt_context* ctx);\n\
             void subscript_test_return_path_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, 1, {HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             static void sample(subscript_rt_context* ctx, int depth, const char* tag) {{\n\
               printf(\"%d %s %llu %llu\\n\", depth, tag,\n\
                 (unsigned long long)subscript_rt_ctx_live_allocations(ctx),\n\
                 (unsigned long long)subscript_rt_automatic_collection_count(ctx));\n\
             }}\n\
             void subscript_test_return_path_run(subscript_rt_context* ctx) {{\n\
             {calls}\
               fflush(stdout);\n\
             }}\n"
        ),
    )
    .expect("write the hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only host hooks, which the C build links.
    let library = [unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hooks], Vec::new())
    }];
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_return_path_setup");
    config.pre_entry_hook = Some("subscript_test_return_path_run");
    let files = [SourceFile::new("return-path.ts", source(shape, 0))];
    let output = run_c_aot_configured(&files, config).expect("the ship tier runs");
    let _ = std::fs::remove_dir_all(&directory);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    assert!(!text.contains("setup-refused"), "{}: {text}", shape.name);
    let read = |depth: i32, tag: &str| -> Sample {
        let line = text
            .lines()
            .find(|line| line.starts_with(&format!("{depth} {tag} ")))
            .unwrap_or_else(|| panic!("{}: no {tag} sample in {text}", shape.name));
        let numbers = line
            .split_whitespace()
            .skip(2)
            .map(|value| value.parse::<u64>().expect("a count"))
            .collect::<Vec<_>>();
        Sample {
            objects: numbers[0],
            collections: numbers[1],
        }
    };
    depths(shape)
        .iter()
        .map(|&depth| {
            let base = read(depth, "base");
            let exit = read(depth, "before-exit");
            ShipFigures {
                depth,
                objects: exit.objects - base.objects,
                collections: exit.collections - base.collections,
            }
        })
        .collect()
}

/// The objects over the base before the outermost exit of one depth in the
/// dev JIT, and the automatic collections and outermost exits of a run of
/// `main` at that depth.
fn dev(shape: &Shape, depth: i32) -> (usize, u64, u64) {
    let files = [SourceFile::new("return-path.ts", source(shape, depth))];
    let mut config = RunConfig::default();
    config.collection = Some((CollectionMode::Automatic, HEADROOM));
    let mut session = ReloadSession::new_configured(&files, config).expect("the session starts");
    session.call_export("settle").expect("settle runs");
    let base = session.live_allocations();
    match session.call_export_with("probe", &[EntryArg::I32(depth)]) {
        Err(RunError::Trap(_)) => {}
        other => panic!("{}: probe({depth}) does not trap: {other:?}", shape.name),
    }
    let objects = session.live_allocations() - base;
    let mut config = RunConfig::default();
    config.collection = Some((CollectionMode::Automatic, HEADROOM));
    config.memory_accounting = true;
    let run = run_jit_configured(&files, config).expect("main runs");
    let accounting = run.memory_accounting.expect("accounting");
    (
        objects,
        accounting.automatic_collections,
        accounting.outermost_exits,
    )
}

/// The finding of one depth of one shape, if it has one. The control keeps
/// one object, the result of the bottom frame. Each other shape runs an
/// automatic collection in the call, and keeps at most `bound` objects.
fn finding(
    shape: &Shape,
    bound: usize,
    objects: u64,
    collected: bool,
    figures: &str,
) -> Option<String> {
    let pass = if shape.name == "control" {
        objects == 1
    } else {
        collected && objects <= bound as u64
    };
    (!pass).then(|| format!("{} bound={bound}: {figures}", shape.name))
}

#[test]
fn a_return_into_a_recursion_reaches_a_poll_in_the_ship_tier() {
    let mut findings = Vec::new();
    for shape in &SHAPES {
        let bound = bound(per_frame(shape));
        for figure in ship(shape) {
            let figures = format!(
                "depth={} objects={} collections={}",
                figure.depth, figure.objects, figure.collections
            );
            eprintln!("ship {} bound={bound}: {figures}", shape.name);
            findings.extend(finding(
                shape,
                bound,
                figure.objects,
                figure.collections > 0,
                &figures,
            ));
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn a_return_into_a_recursion_reaches_a_poll_in_the_dev_jit() {
    let mut findings = Vec::new();
    for shape in &SHAPES {
        let bound = bound(per_frame(shape));
        for depth in depths(shape) {
            let (objects, collections, exits) = dev(shape, depth);
            let figures =
                format!("depth={depth} objects={objects} collections={collections} exits={exits}");
            eprintln!("dev {} bound={bound}: {figures}", shape.name);
            // An outermost exit runs one automatic collection at most, so a
            // count over the exit count is a collection at a poll.
            findings.extend(finding(
                shape,
                bound,
                objects as u64,
                collections > exits,
                &figures,
            ));
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

/// A recursion that reads a managed result of the call after the new poll:
/// an object, a string, an array, and an object that a handler reads.
const ROOTS: &str = r#"
class P { x: i32 = 0; }
function build(depth: i32): P {
  if (depth === 0) { return new P(); }
  const p: P = build(depth - 1);
  const q: P = new P();
  q.x = p.x + 1;
  return q;
}
function text(depth: i32): string {
  if (depth === 0) { return "a"; }
  const s: string = text(depth - 1);
  const t: string = `${depth}`;
  return s.length > 40 ? t : s + t;
}
function list(depth: i32): i32[] {
  if (depth === 0) { return [0]; }
  const a: i32[] = list(depth - 1);
  const b: P = new P();
  b.x = a.length;
  a.push(b.x + a[a.length - 1]);
  return a;
}
function dive(depth: i32): P {
  if (depth === 0) { throw new Error("bottom"); }
  const keep: P = new P();
  keep.x = depth;
  try {
    return dive(depth - 1);
  } catch (e) {
    const p: P = new P();
    p.x = keep.x;
    if (e instanceof Error) { print(`${e.message}:${p.x}`); }
    throw e;
  }
}
export function main(): void {
  const p: P = build(12);
  const s: string = text(12);
  const a: i32[] = list(12);
  print(`${p.x} ${s} ${a.length} ${a[a.length - 1]}`);
  try { dive(3); } catch (e) { if (e instanceof Error) { print(`top ${e.message}`); } }
}
"#;

/// The ship tier with the collection mode set before the initializer.
fn ship_output(source: &str, mode: i32) -> String {
    let directory = scratch(&format!("roots-{mode}"));
    let hooks = directory.join("return_path_roots.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             void subscript_test_return_path_mode(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, {mode}, {HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n"
        ),
    )
    .expect("write the hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only a host hook, which the C build links.
    let library = [unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hooks], Vec::new())
    }];
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_return_path_mode");
    let files = [SourceFile::new("return-path-roots.ts", source)];
    let output = run_c_aot_configured(&files, config).expect("the ship tier runs");
    let _ = std::fs::remove_dir_all(&directory);
    String::from_utf8(output.stdout).expect("utf-8 output")
}

/// The dev JIT with a collection mode. Memory accounting keeps the run in
/// the test process, as the other runs of this file.
fn dev_output(source: &str, mode: CollectionMode) -> String {
    let files = [SourceFile::new("return-path-roots.ts", source)];
    let mut config = RunConfig::default();
    config.collection = Some((mode, HEADROOM));
    config.memory_accounting = true;
    let run = run_jit_configured(&files, config).expect("the dev JIT runs");
    String::from_utf8(run.stdout).expect("utf-8 output")
}

#[test]
fn a_value_that_the_continuation_reads_is_rooted_at_the_new_poll() {
    let files = [SourceFile::new("return-path-roots.ts", ROOTS)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    // Each recursion has a poll directly after its call into the cycle, and
    // `dive` has a poll in its handler.
    for name in ["build", "text", "list", "dive"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.source_name == name)
            .unwrap_or_else(|| panic!("function {name} is missing"));
        let return_path_polls = function
            .blocks
            .iter()
            .filter(|block| block.id != function.entry)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| instruction.kind == l::InstructionKind::CollectionPoll)
            .count();
        assert!(return_path_polls > 0, "{name}");
    }
    let (reference, _) = interpret_with_collection(&module, CollectionMode::Off, HEADROOM)
        .expect("the interpreter runs with mode 0");
    let reference = String::from_utf8(reference).expect("utf-8 output");
    let (each_poll, run) = interpret_with_collection(&module, CollectionMode::EachPoll, HEADROOM)
        .expect("the interpreter runs with mode 2");
    assert!(run.automatic_collections > 0);
    assert_eq!(
        String::from_utf8(each_poll).expect("utf-8"),
        reference,
        "interpreter"
    );
    assert_eq!(
        dev_output(ROOTS, CollectionMode::Off),
        reference,
        "dev JIT mode 0"
    );
    assert_eq!(
        dev_output(ROOTS, CollectionMode::EachPoll),
        reference,
        "dev JIT mode 2"
    );
    assert_eq!(ship_output(ROOTS, 0), reference, "ship tier mode 0");
    assert_eq!(ship_output(ROOTS, 2), reference, "ship tier mode 2");
}

/// The poll sites of each benchmark workload (`compiler.md` §351
/// acceptance 5): the counts at the contract pin, with the coercion fact of
/// rule 9 on both sides.
const WORKLOAD_POLLS: [(&str, usize); 7] = [
    ("tree", 2),
    ("callbacks", 2),
    ("sort", 1),
    ("particles", 1),
    ("queen", 0),
    ("fib-recursive", 0),
    ("collect", 2),
];

#[test]
fn the_workloads_keep_their_poll_sites() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/workloads/subscript");
    for (name, expected) in WORKLOAD_POLLS {
        let path = directory.join(format!("{name}.ts"));
        let text = std::fs::read_to_string(&path).expect("read the workload");
        let files = [SourceFile::new(format!("{name}.ts"), text)];
        let hir = corpus::check_program(&files).expect("the workload checks");
        let module = lower_module(&hir).expect("the workload lowers");
        let polls = module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| instruction.kind == l::InstructionKind::CollectionPoll)
            .count();
        assert_eq!(polls, expected, "{name}");
    }
}

/// The async shapes (`compiler.md` §351 rule 12): the return chain of an
/// async recursion runs as resumes in one drain of the scheduler, with no
/// poll in the resumed code. Each level allocates five objects after its
/// await, more than the bytes of the child frame that it releases, so the
/// requested bytes rise during the drain (§346 rule 1). With one object
/// for each level, the release of the child frame offsets the object, and
/// no collection is due in the drain. The forms: an await
/// of the call, an await of a held handle, and a level that suspends once
/// before it calls down.
const ASYNC_COMMON: &str = r#"
class P { a: i64 = 0; b: i64 = 0; c: i64 = 0; d: i64 = 0; }
let depthArgument: i32 = 0;
let stopArgument: i32 = 0;
export function configure(depth: i32, stop: i32): void {
  depthArgument = depth;
  stopArgument = stop;
}
export async function start(): Promise<void> {
  await walk(depthArgument);
  if (stopArgument !== 0) {
    const slot: FixedArray<i32, 1> = [0];
    print(`${slot[depthArgument]}`);
  }
}
export function settle(): void { Context.collect(); }
export async function main(): Promise<void> { await walk(DEPTH); }
"#;

const ASYNC_SHAPES: [Shape; 3] = [
    Shape {
        name: "async",
        source: r#"
async function walk(depth: i32): Promise<P> {
  if (depth === 0) { return new P(); }
  await walk(depth - 1);
  const g: P = new P();
  const h2: P = new P();
  const h3: P = new P();
  const h4: P = new P();
  g.a = h2.a + h3.a + h4.a + (depth as i64);
  return new P();
}
"#,
    },
    Shape {
        name: "async-held",
        source: r#"
async function walk(depth: i32): Promise<P> {
  if (depth === 0) { return new P(); }
  const h: Promise<P> = walk(depth - 1);
  await h;
  const g: P = new P();
  const h2: P = new P();
  const h3: P = new P();
  const h4: P = new P();
  g.a = h2.a + h3.a + h4.a + (depth as i64);
  return new P();
}
"#,
    },
    Shape {
        name: "async-suspend",
        source: r#"
async function walk(depth: i32): Promise<P> {
  if (depth === 0) { return new P(); }
  await Context.suspend();
  await walk(depth - 1);
  const g: P = new P();
  const h2: P = new P();
  const h3: P = new P();
  const h4: P = new P();
  g.a = h2.a + h3.a + h4.a + (depth as i64);
  return new P();
}
"#,
    },
];

fn async_source(shape: &Shape, depth: i32) -> String {
    format!(
        "const DEPTH: i32 = {depth};\n{ASYNC_COMMON}{}",
        shape.source
    )
}

/// The payload capacity of a coroutine frame of an async shape in the ship
/// tier: the most that the frame request can be, and the live bytes of one
/// frame there. The ship test reads it from the live frames.
const FRAME_CAPACITY: usize = 48;

/// The payload capacity of each other object of an async shape in the ship
/// tier. The ship test reads it from the live objects.
const OBJECT_CAPACITY: usize = 48;

/// The bound of the objects over the base before the outermost exit of an
/// async shape.
///
/// Each level of the return chain requests five `P` objects and releases
/// the frame of its child, and the release subtracts the request of that
/// frame (§346 rule 1). So the net requested bytes of one level are at
/// least `n = 5 * SIZE - FRAME_CAPACITY`.
///
/// After the last collection of the drain, `j` levels run. The limit is the
/// larger of the headroom and the live bytes at that collection (§346 rule
/// 2). The live bytes are at most `(j + 2) * FRAME_CAPACITY +
/// OBJECT_CAPACITY`: the frames of the `j` levels, the frames of `start` and
/// of the completed child, and the result of that child. The drain polls
/// before each job, so the first `j - 1` levels request at most the limit.
/// Thus `j <= HEADROOM / n + 1` if the headroom is the limit, and `j <= (n +
/// 2 * FRAME_CAPACITY + OBJECT_CAPACITY) / (n - FRAME_CAPACITY)` if the live
/// bytes are the limit. The second form needs `n > FRAME_CAPACITY`: else the
/// limit grows with the depth. The bound is `j * per_frame` objects, and the
/// result of the completed child.
///
/// The present figures: `n = 112`, `j = 4`. The dev JIT exposes no frame
/// size to a test, so its test applies the ship figures. If no flag fires
/// in the drain, its check `collections > exits` fails.
fn async_bound(per_frame: usize) -> usize {
    let net = 5 * SIZE - FRAME_CAPACITY;
    assert!(
        net > FRAME_CAPACITY,
        "the net bytes {net} of one level do not exceed a frame"
    );
    let headroom_levels = HEADROOM / net + 1;
    let live_levels = (net + 2 * FRAME_CAPACITY + OBJECT_CAPACITY) / (net - FRAME_CAPACITY);
    headroom_levels.max(live_levels) * per_frame + 1
}

/// Runs each depth of an async shape in the ship tier, in one build. The
/// hook brackets the kick and each step with one script entry, so no step
/// is an outermost exit, and samples before the exit. It also returns the
/// largest payload capacity of a live frame after the kick, and of a live
/// object other than a frame before the exit.
fn async_ship(shape: &Shape) -> (Vec<ShipFigures>, u64, u64) {
    let directory = scratch(shape.name);
    let hooks = directory.join("async_return_path_hooks.c");
    let calls = DEPTHS
        .iter()
        .map(|depth| {
            format!(
                "  subscript_export_settle(ctx);\n  sample(ctx, {depth}, \"base\");\n\
                 \x20 subscript_rt_ctx_enter_script(ctx);\n\
                 \x20 subscript_export_configure(ctx, {depth}, 0);\n\
                 \x20 subscript_export_start(ctx);\n\
                 \x20 sample(ctx, {depth}, \"kicked\");\n\
                 \x20 capacities(ctx, {depth}, \"frame\", 1);\n\
                 \x20 while (subscript_rt_ctx_async_pending(ctx) != 0) {{ (void)subscript_rt_ctx_async_step(ctx); }}\n\
                 \x20 sample(ctx, {depth}, \"before-exit\");\n\
                 \x20 capacities(ctx, {depth}, \"object\", 0);\n\
                 \x20 subscript_rt_ctx_exit_script(ctx);\n"
            )
        })
        .collect::<String>();
    std::fs::write(
        &hooks,
        format!(
            "#include <stdint.h>\n\
             #include <stdio.h>\n\
             #include \"program.h\"\n\
             uint64_t subscript_rt_automatic_collection_count(const subscript_rt_context* ctx);\n\
             void subscript_test_return_path_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, 1, {HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             static uint64_t most_frame;\n\
             static uint64_t most_object;\n\
             static void capacity(void* userdata, uint32_t class_id, uint32_t pos_id, uint64_t bytes) {{\n\
               (void)userdata; (void)pos_id;\n\
               uint64_t* most = class_id == 0xFFFFFF04u ? &most_frame : &most_object;\n\
               if (bytes > *most) {{ *most = bytes; }}\n\
             }}\n\
             static void capacities(subscript_rt_context* ctx, int depth, const char* tag, int frames) {{\n\
               most_frame = 0; most_object = 0;\n\
               (void)subscript_rt_ctx_visit_live_allocations(ctx, capacity, NULL);\n\
               printf(\"%d %s %llu 0\\n\", depth, tag,\n\
                 (unsigned long long)(frames ? most_frame : most_object));\n\
             }}\n\
             static void sample(subscript_rt_context* ctx, int depth, const char* tag) {{\n\
               printf(\"%d %s %llu %llu\\n\", depth, tag,\n\
                 (unsigned long long)subscript_rt_ctx_live_allocations(ctx),\n\
                 (unsigned long long)subscript_rt_automatic_collection_count(ctx));\n\
             }}\n\
             void subscript_test_return_path_run(subscript_rt_context* ctx) {{\n\
             {calls}\
               fflush(stdout);\n\
             }}\n"
        ),
    )
    .expect("write the hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only host hooks, which the C build links.
    let library = [unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hooks], Vec::new())
    }];
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_return_path_setup");
    config.pre_entry_hook = Some("subscript_test_return_path_run");
    let files = [SourceFile::new("return-path.ts", async_source(shape, 0))];
    let output = run_c_aot_configured(&files, config).expect("the ship tier runs");
    let _ = std::fs::remove_dir_all(&directory);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    assert!(!text.contains("setup-refused"), "{}: {text}", shape.name);
    let read = |depth: i32, tag: &str| -> Sample {
        let line = text
            .lines()
            .find(|line| line.starts_with(&format!("{depth} {tag} ")))
            .unwrap_or_else(|| panic!("{}: no {tag} sample in {text}", shape.name));
        let numbers = line
            .split_whitespace()
            .skip(2)
            .map(|value| value.parse::<u64>().expect("a count"))
            .collect::<Vec<_>>();
        Sample {
            objects: numbers[0],
            collections: numbers[1],
        }
    };
    let most = |tag: &str| {
        DEPTHS
            .iter()
            .map(|&depth| read(depth, tag).objects)
            .max()
            .unwrap_or(0)
    };
    let (frame, object) = (most("frame"), most("object"));
    let figures = DEPTHS
        .iter()
        .map(|&depth| {
            let base = read(depth, "base");
            let kicked = read(depth, "kicked");
            let exit = read(depth, "before-exit");
            ShipFigures {
                depth,
                objects: exit.objects - base.objects,
                collections: exit.collections - kicked.collections,
            }
        })
        .collect();
    (figures, frame, object)
}

/// The objects over the base before the outermost exit of the drain in the
/// dev JIT, and the automatic collections and outermost exits of a run of
/// `main`. `start` stops on an index trap after the chain returns, so the
/// exit of the last drain runs no collection (§342 rule 8).
fn async_dev(shape: &Shape, depth: i32) -> (usize, u64, u64) {
    let files = [SourceFile::new(
        "return-path.ts",
        async_source(shape, depth),
    )];
    let mut config = RunConfig::default();
    config.collection = Some((CollectionMode::Automatic, HEADROOM));
    let mut session = ReloadSession::new_configured(&files, config).expect("the session starts");
    session.call_export("settle").expect("settle runs");
    let base = session.live_allocations();
    session
        .call_export_with("configure", &[EntryArg::I32(depth), EntryArg::I32(1)])
        .expect("configure runs");
    session.call_export("start").expect("start runs");
    let trapped = loop {
        match session.async_step() {
            Ok(0) => break false,
            Ok(_) => {}
            Err(RunError::Trap(_)) => break true,
            Err(other) => panic!("{}: {other:?}", shape.name),
        }
    };
    assert!(trapped, "{}: run({depth}) does not trap", shape.name);
    let objects = session.live_allocations() - base;
    let mut config = RunConfig::default();
    config.collection = Some((CollectionMode::Automatic, HEADROOM));
    config.memory_accounting = true;
    let run = run_jit_configured(&files, config).expect("main runs");
    let accounting = run.memory_accounting.expect("accounting");
    (
        objects,
        accounting.automatic_collections,
        accounting.outermost_exits,
    )
}

#[test]
fn an_async_return_chain_reaches_a_poll_in_the_ship_tier() {
    let mut findings = Vec::new();
    for shape in &ASYNC_SHAPES {
        let bound = async_bound(per_frame_of(&async_source(shape, 0)));
        let (figures, frame, object) = async_ship(shape);
        eprintln!("ship {} frame={frame} object={object}", shape.name);
        // The premise of the bound: the capacities that it assumes.
        assert!(
            frame > 0 && frame <= FRAME_CAPACITY as u64,
            "{}: frame capacity {frame}",
            shape.name
        );
        assert!(
            object > 0 && object <= OBJECT_CAPACITY as u64,
            "{}: object capacity {object}",
            shape.name
        );
        for figure in figures {
            let figures = format!(
                "depth={} objects={} collections={}",
                figure.depth, figure.objects, figure.collections
            );
            eprintln!("ship {} bound={bound}: {figures}", shape.name);
            findings.extend(finding(
                shape,
                bound,
                figure.objects,
                figure.collections > 0,
                &figures,
            ));
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn an_async_return_chain_reaches_a_poll_in_the_dev_jit() {
    let mut findings = Vec::new();
    for shape in &ASYNC_SHAPES {
        let bound = async_bound(per_frame_of(&async_source(shape, 0)));
        for depth in DEPTHS {
            let (objects, collections, exits) = async_dev(shape, depth);
            let figures =
                format!("depth={depth} objects={objects} collections={collections} exits={exits}");
            eprintln!("dev {} bound={bound}: {figures}", shape.name);
            // Each step of the suspend form is an outermost exit, so the
            // count of that run does not tell a poll from an exit. The ship
            // tier counts the collections of the drain itself.
            let collected = shape.name == "async-suspend" || collections > exits;
            findings.extend(finding(shape, bound, objects as u64, collected, &figures));
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

/// Tasks that hold managed values across a suspension and read them after
/// the resume, with allocations between, and an async recursion that reads
/// a managed result after its await (`compiler.md` §351 rule 12). Mode 2
/// collects at each poll of the drain, between each two jobs.
const ASYNC_ROOTS: &str = r#"
class Q { x: i32 = 0; }
async function task(id: i32): Promise<string> {
  const q: Q = new Q();
  q.x = id;
  const s: string = `t${id}`;
  const a: i32[] = [id, id + 1];
  await Context.suspend();
  const g: Q = new Q();
  g.x = q.x * 2;
  await Context.suspend();
  return `${s}:${q.x}:${a[1]}:${g.x}`;
}
async function chain(depth: i32): Promise<string> {
  if (depth === 0) { return "end"; }
  const keep: Q = new Q();
  keep.x = depth;
  const rest: string = await chain(depth - 1);
  const t: string = `${keep.x}`;
  return rest.length > 30 ? t : rest + t;
}
export async function main(): Promise<void> {
  const a: Promise<string> = task(1);
  const b: Promise<string> = task(2);
  const c: Promise<string> = task(3);
  print(await a);
  print(await b);
  print(await c);
  print(await chain(12));
}
"#;

#[test]
fn a_value_that_a_task_holds_is_rooted_at_the_drain_poll() {
    let files = [SourceFile::new("return-path-roots.ts", ASYNC_ROOTS)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (reference, _) = interpret_with_collection(&module, CollectionMode::Off, HEADROOM)
        .expect("the interpreter runs with mode 0");
    let reference = String::from_utf8(reference).expect("utf-8 output");
    assert_eq!(reference.lines().count(), 4, "{reference}");
    let (each_poll, run) = interpret_with_collection(&module, CollectionMode::EachPoll, HEADROOM)
        .expect("the interpreter runs with mode 2");
    assert!(run.automatic_collections > 0);
    assert_eq!(
        String::from_utf8(each_poll).expect("utf-8"),
        reference,
        "interpreter"
    );
    assert_eq!(
        dev_output(ASYNC_ROOTS, CollectionMode::EachPoll),
        reference,
        "dev JIT mode 2"
    );
    assert_eq!(ship_output(ASYNC_ROOTS, 2), reference, "ship tier mode 2");
}

/// The interpreter's drain serves the flag between two jobs
/// (`compiler.md` §351 rule 12). In the plain form and the held form, one
/// drain runs the whole return chain, and an outermost exit runs one
/// automatic collection at most, so a count over the exit count is a
/// collection at a poll inside the drain. Each step of the suspend form is
/// an outermost exit, so this count does not separate a poll from an exit
/// there; `a_drain_polls_between_two_jobs_only` in the interpreter's unit
/// tests counts each poll of a drain. The depth is 16: the interpreter runs
/// each level on the host stack, and depth 32 of the held form exceeds the
/// stack of a debug test thread.
#[test]
fn an_async_return_chain_reaches_a_poll_in_the_interpreter() {
    let mut findings = Vec::new();
    for shape in ASYNC_SHAPES
        .iter()
        .filter(|shape| shape.name != "async-suspend")
    {
        let files = [SourceFile::new("return-path.ts", async_source(shape, 16))];
        let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
        let (_, run) = interpret_with_collection(&module, CollectionMode::Automatic, HEADROOM)
            .expect("the interpreter runs");
        let figures = format!(
            "{} collections={} exits={}",
            shape.name, run.automatic_collections, run.outermost_exits
        );
        eprintln!("interpreter {figures}");
        if run.automatic_collections <= run.outermost_exits {
            findings.push(figures);
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}
