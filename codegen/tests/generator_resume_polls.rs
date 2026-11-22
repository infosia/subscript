//! Polls on the return from a generator resume and at the resume points of
//! a generator (`compiler.md` §352, acceptance 1, 2, 4, 5, and 6, and rule
//! 9).
//!
//! Each shape runs at depths 128 and 256 in automatic mode with a minimum
//! headroom of 64 bytes, in the ship tier and the dev JIT. A debug build of
//! the ship program runs on a main-thread stack of 1 MiB on one host
//! (§351.4 item 6), and each level of a shape nests a runtime frame and a
//! generated resume function. The sample is before the outermost exit,
//! because the exit collects (§342 rule 8).
//!
//! - **Ship tier.** A pre-entry host hook collects, samples, brackets the
//!   export `run(depth)` with one script entry, and samples between the
//!   export and the exit. The program calls the foreign function
//!   `markBottom` at the bottom of the recursion, after the last allocation
//!   of the descent and before the first allocation of the return path.
//!   The hook reads the automatic collection count there. The count before
//!   the exit minus the count at the bottom is the collections of the
//!   return path, so a run with collections on the descent only fails.
//! - **Dev JIT.** A reload session calls `settle`, which collects, and
//!   samples. It then calls `probe(depth)`, which runs the shape and then
//!   stops on an index trap, so the live count after the call is the live
//!   count before the exit. The bottom marker does nothing there, so the
//!   test has no count of the collections after the bottom, and a count
//!   from the start of the call passes with collections on the descent
//!   only. The dev JIT test relies on the object bound alone. That is
//!   enough: with only the poll that a shape names removed, the objects of
//!   each shape grow with the depth, over the bound ([`SHAPES`]).
//!
//! The shapes of the descent through resume points (rule 9) sample the
//! live objects at a bottom marker instead, with a minimum headroom of 128
//! KiB: the design is at [`down_bound`].
//!
//! Cost: one ship-tier C build for each shape, and two dev JIT compiles for
//! each shape and depth. Each root test runs one program in the interpreter two
//! times, in the dev JIT two times, and in two ship-tier builds. The corpus
//! test checks and lowers each accept entry with a generator function,
//! places its polls two times more (the identity map and the resume fact
//! masked), and builds one call graph; it runs no code. Wall times, debug and release: ship tier 1.0 s and 0.8 s, dev JIT
//! 0.6 s and 0.05 s, descent in the ship tier 1.0 s and 0.7 s, descent in
//! the dev JIT 2.1 s and 0.15 s, roots 0.6 s and 0.4 s, held values 0.7 s
//! and 0.4 s, corpus 1.4 s and 0.2 s, structure 0.02 s and under 0.01 s.

use std::collections::BTreeMap;

use subscript_codegen::interpreter::interpret_with_collection;
use subscript_codegen::lir::collection_poll::place_collection_polls_with;
use subscript_codegen::lir::lower_module;
use subscript_codegen::lir::script_entry::{
    instruction_script_entry, terminator_script_entry, ScriptEntry, ScriptMethods,
};
use subscript_compiler::{check_program, lir as l, SourceFile};
use subscript_runtime::context::CollectionMode;

#[allow(dead_code)]
mod corpus;
#[allow(dead_code)]
#[path = "support/return_path.rs"]
mod return_path;

use return_path::{
    dev_marker, dev_output, dev_run, per_frame_of, ship_output, ship_run, HEADROOM, MARKER, SIZE,
};

/// The depths of each shape.
const DEPTHS: [i32; 2] = [128, 256];

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

/// A shape: its name, its source, the function with the resume whose poll
/// the shape tests, the objects that one level allocates on the return
/// path, and the largest payload capacity of a live allocation at the
/// bottom in the ship tier (a generator frame or a `P`), which the ship
/// test reads at the bottom marker.
struct Shape {
    name: &'static str,
    source: &'static str,
    resumer: &'static str,
    objects: usize,
    capacity: usize,
}

/// The shapes of acceptance 1 and 2. Each level allocates its objects
/// after the resume of its child returns. `fixture` is the program of the
/// problem of §352; `parameter` resumes a handle that it receives;
/// `for-of` iterates its child; `delegate` passes each value of its child
/// up with `for (const v of child) { yield v; }` and allocates when its
/// child completes. The frame of `delegate` has a
/// capacity of 112 bytes, so a level allocates ten objects: with five, the
/// net bytes of a level are under the frame, and the limit grows with the
/// depth (the bound below).
///
/// The poll after the resume in its `resumer` is the only poll on the
/// return path of each shape. With no poll after the resume, each level
/// keeps its objects, so the count follows the depth and passes the bound
/// in the ship tier and the dev JIT, and the ship tier runs no collection
/// after the bottom. In `parameter`, `step` has no poll after its call of
/// `drive`, so the poll after the resume in `drive` is the only poll on the
/// return path.
const SHAPES: [Shape; 4] = [
    Shape {
        name: "fixture",
        resumer: "step",
        objects: 5,
        capacity: 48,
        source: r#"
function* child(d: i32): Generator<i32> {
  if (d >= 0) { step(d); return; }
  yield 1;
}
function step(d: i32): void {
  if (d > 0) {
    const it = child(d - 1);
    it.next();
  } else { markBottom(); }
  new P(); new P(); new P(); new P(); new P();
}
export function run(d: i32): void { step(d); }
"#,
    },
    Shape {
        name: "parameter",
        resumer: "drive",
        objects: 5,
        capacity: 48,
        source: r#"
function* child(d: i32): Generator<i32> {
  if (d >= 0) { step(d); return; }
  yield 1;
}
function drive(it: Generator<i32>): void {
  it.next();
  new P(); new P(); new P(); new P(); new P();
}
function step(d: i32): void {
  if (d > 0) { drive(child(d - 1)); } else { markBottom(); }
}
export function run(d: i32): void { step(d); }
"#,
    },
    Shape {
        name: "for-of",
        resumer: "walk",
        objects: 5,
        capacity: 48,
        source: r#"
function* walk(d: i32): Generator<i32> {
  if (d > 0) { for (const x of walk(d - 1)) { } } else { markBottom(); }
  new P(); new P(); new P(); new P(); new P();
  if (d < 0) { yield 1; }
}
export function run(d: i32): void { for (const x of walk(d)) { } }
"#,
    },
    Shape {
        name: "delegate",
        resumer: "pass",
        objects: 10,
        capacity: 112,
        source: r#"
function* pass(d: i32): Generator<i32> {
  if (d === 0) { markBottom(); yield 1; return; }
  for (const v of pass(d - 1)) { yield v; }
  new P(); new P(); new P(); new P(); new P();
  new P(); new P(); new P(); new P(); new P();
}
export function run(d: i32): void { for (const v of pass(d)) { } }
"#,
    },
];

fn files(shape: &Shape, depth: i32) -> [SourceFile; 2] {
    [
        SourceFile::ambient("return_path_marker.d.ts", MARKER),
        SourceFile::new(
            "generator-resume.ts",
            format!("const DEPTH: i32 = {depth};\n{COMMON}{}", shape.source),
        ),
    ]
}

/// The bound of the objects over the base before the outermost exit.
///
/// Each level of the return path requests `objects` objects of `SIZE`
/// bytes and releases the frame of its child, and the release subtracts the
/// request of that frame (§346 rule 1). So the net requested bytes of one
/// level are at least `n = objects * SIZE - capacity`.
///
/// Let the last collection of the call run with `r` levels of the return
/// path left. The limit is the larger of the headroom and the live bytes
/// at that collection (§346 rule 2). The live bytes are at most `(r + 3) *
/// capacity`: the frames of the `r` levels, the frame of the child that
/// completed, the value that it passed up, and one `P` of the level that
/// polls. The `r` levels run with no collection, and each level has a poll
/// after its resume, so the first `r - 1` levels request at most the limit.
/// Thus `r <= HEADROOM / n + 1` if the headroom is the limit, and
/// `r <= (n + 3 * capacity) / (n - capacity)` if the live bytes are the
/// limit. The second form needs `n > capacity`: else the limit grows with
/// the depth.
/// The bound is `r + 1` levels of `per_frame` objects: the level that set
/// the flag before the collection runs to its poll. The bound does not
/// depend on the depth.
fn bound(shape: &Shape) -> usize {
    let per_frame = per_frame_of(&files(shape, 0));
    let net = shape.objects * SIZE - shape.capacity;
    assert!(
        net > shape.capacity,
        "{}: the net bytes {net} of one level do not exceed a frame",
        shape.name
    );
    let headroom_levels = HEADROOM / net + 1;
    let live_levels = (net + 3 * shape.capacity) / (net - shape.capacity);
    (headroom_levels.max(live_levels) + 1) * per_frame
}

#[test]
fn a_return_from_a_generator_resume_reaches_a_poll_in_the_ship_tier() {
    let mut findings = Vec::new();
    for shape in &SHAPES {
        let bound = bound(shape);
        for figure in ship_run(shape.name, &files(shape, 0), &DEPTHS, true) {
            let after_bottom = figure.after_bottom.unwrap_or(0);
            let capacity = figure.bottom_capacity.unwrap_or(0);
            let figures = format!(
                "depth={} objects={} collections={} after-bottom={after_bottom} \
                 capacity={capacity}",
                figure.depth, figure.objects, figure.collections
            );
            eprintln!("ship {} bound={bound}: {figures}", shape.name);
            // The premise of the bound: the capacity that it assumes.
            assert!(
                capacity > 0 && capacity <= shape.capacity as u64,
                "{}: capacity {capacity}",
                shape.name
            );
            if after_bottom == 0 || figure.objects > bound as u64 {
                findings.push(format!("{} bound={bound}: {figures}", shape.name));
            }
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn a_return_from_a_generator_resume_reaches_a_poll_in_the_dev_jit() {
    let library = [dev_marker()];
    let mut findings = Vec::new();
    for shape in &SHAPES {
        let bound = bound(shape);
        for depth in DEPTHS {
            let (objects, collections, exits) =
                dev_run(shape.name, &files(shape, depth), depth, &library);
            let figures =
                format!("depth={depth} objects={objects} collections={collections} exits={exits}");
            eprintln!("dev {} bound={bound}: {figures}", shape.name);
            // The collection count includes the collections of the descent,
            // so the test reads the object bound only (module doc).
            if objects > bound {
                findings.push(format!("{} bound={bound}: {figures}", shape.name));
            }
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

fn lower(name: &str, source: &str) -> l::Module {
    let files = [
        SourceFile::ambient("return_path_marker.d.ts", MARKER),
        SourceFile::new(name, source),
    ];
    lower_module(&check_program(&files).expect("checks")).expect("lowers")
}

fn function<'m>(module: &'m l::Module, name: &str) -> &'m l::Function {
    module
        .functions
        .iter()
        .find(|function| function.source_name == name)
        .unwrap_or_else(|| panic!("function {name} is missing"))
}

/// True if the instruction resumes a generator: `next()`.
fn is_resume(instruction: &l::Instruction) -> bool {
    matches!(
        &instruction.kind,
        l::InstructionKind::Call(l::CallTarget {
            kind: l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::GeneratorNext),
            ..
        })
    )
}

/// For each resume of the function: true if a poll directly follows it.
fn polls_after_resumes(function: &l::Function) -> Vec<bool> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, instruction)| is_resume(instruction))
                .map(|(index, _)| {
                    block.instructions.get(index + 1).map(|next| &next.kind)
                        == Some(&l::InstructionKind::CollectionPoll)
                })
        })
        .collect()
}

/// Each shape has a poll directly after its resume. The control: a
/// recursion resumes a generator that does not reach the recursion, so the
/// body is not in the cycle of the resume, and the resume has no poll after
/// it (§352 rule 4).
#[test]
fn each_shape_has_a_poll_after_its_resume_and_the_control_has_none() {
    for shape in &SHAPES {
        let source = format!("const DEPTH: i32 = 0;\n{COMMON}{}", shape.source);
        let module = lower("generator-resume.ts", &source);
        let after = polls_after_resumes(function(&module, shape.resumer));
        assert!(
            !after.is_empty() && after.iter().all(|poll| *poll),
            "{}: {after:?}",
            shape.name
        );
    }
    let control = lower(
        "generator-control.ts",
        r#"
class P { a: i64 = 0; }
function* leaf(n: i32): Generator<i32> { yield n; }
function rec(d: i32): i32 {
  if (d === 0) { return 0; }
  const it: Generator<i32> = leaf(d);
  const v: i32 = it.next().value;
  const p: P = new P();
  p.a = v as i64;
  return rec(d - 1) + v;
}
export function main(): void { print(`${rec(3)}`); }
"#,
    );
    assert_eq!(polls_after_resumes(function(&control, "rec")), vec![false]);
}

/// A generator yields an object, a string, and an array that have no other
/// holder, and the caller reads them through the result of `next()` after
/// the poll after the resume (`compiler.md` §352 rule 6). The depth is 8:
/// the interpreter runs each resume on the host stack, and depth 12 ends
/// the stack of a debug test thread.
const ROOTS: &str = r#"
class Q { x: i32 = 0; }
function fresh(n: i32): Q { const q: Q = new Q(); q.x = n; return q; }
function* make(d: i32): Generator<Q> { const n: i32 = build(d); yield fresh(n); }
function build(d: i32): i32 {
  if (d === 0) { return 1; }
  const it: Generator<Q> = make(d - 1);
  const r = it.next();
  const pad: Q = new Q();
  pad.x = r.value.x;
  return r.value.x + pad.x;
}
function* text(d: i32): Generator<string> { const s: string = word(d); yield `${s}${d % 10}`; }
function word(d: i32): string {
  if (d === 0) { return "a"; }
  const it: Generator<string> = text(d - 1);
  const r = it.next();
  const pad: string = `${d}`;
  return r.value.length > 40 ? pad : r.value + pad;
}
function* list(d: i32): Generator<i32[]> { const n: i32 = count(d); yield [n, n + 1]; }
function count(d: i32): i32 {
  if (d === 0) { return 0; }
  const it: Generator<i32[]> = list(d - 1);
  const r = it.next();
  const pad: i32[] = [d];
  return r.value[1] + pad[0];
}
export function main(): void {
  print(`${build(8)} ${word(8)} ${count(8)}`);
}
"#;

#[test]
fn a_value_that_a_resume_returns_is_rooted_at_the_new_poll() {
    let files = [SourceFile::new("generator-roots.ts", ROOTS)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    for name in ["build", "word", "count"] {
        assert_eq!(
            polls_after_resumes(function(&module, name)),
            vec![true],
            "{name}"
        );
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

/// The poll sites of each function, by name, in function order.
fn poll_sites(module: &l::Module) -> Vec<(String, usize)> {
    module
        .functions
        .iter()
        .map(|function| {
            let polls = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter(|instruction| instruction.kind == l::InstructionKind::CollectionPoll)
                .count();
            (function.source_name.clone(), polls)
        })
        .collect()
}

/// For each function: true if a cycle of the call graph through the
/// function contains a resume edge. The graph is built here from the
/// script entry fact, with the edges of `compiler.md` §352 rules 1 to 3: a
/// resume edge goes from a function with the fact "each generator body" to
/// each generator function, so the function is in such a cycle if it
/// reaches a function with a resume and a generator reaches it.
fn in_a_cycle_through_a_resume(module: &l::Module) -> Vec<bool> {
    let count = module.functions.len();
    let methods = ScriptMethods::of_module(module);
    let taken = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            l::InstructionKind::FunctionRef(id) | l::InstructionKind::MakeClosure(id) => {
                Some(id.0 as usize)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let generators = (0..count)
        .filter(|index| module.functions[*index].is_generator)
        .collect::<Vec<_>>();
    let mut edges = vec![Vec::new(); count];
    let mut resumes = vec![false; count];
    for (index, function) in module.functions.iter().enumerate() {
        let facts = function
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .map(|instruction| instruction_script_entry(&methods, function, instruction))
                    .chain([terminator_script_entry(
                        &methods,
                        function,
                        &block.terminator,
                    )])
            })
            .collect::<Vec<_>>();
        for fact in facts {
            match fact {
                ScriptEntry::Function(id) | ScriptEntry::AsyncFunction(id) => {
                    edges[index].push(id.0 as usize);
                }
                ScriptEntry::AddressTaken => edges[index].extend(&taken),
                ScriptEntry::GeneratorBody => {
                    resumes[index] = true;
                    edges[index].extend(&generators);
                }
                _ => {}
            }
        }
    }
    // reaches[a][b]: a path of zero edges or more from `a` to `b`.
    let reaches = (0..count)
        .map(|start| {
            let mut seen = vec![false; count];
            let mut work = vec![start];
            while let Some(next) = work.pop() {
                if next < count && !std::mem::replace(&mut seen[next], true) {
                    work.extend(&edges[next]);
                }
            }
            seen
        })
        .collect::<Vec<_>>();
    (0..count)
        .map(|function| {
            (0..count).any(|resumer| resumes[resumer] && reaches[function][resumer])
                && generators
                    .iter()
                    .any(|&generator| reaches[generator][function])
        })
        .collect()
}

/// Masking the resume fact removes polls only from functions that are in a
/// cycle through a resume edge (acceptance 5), and the count of poll sites
/// over the accept corpus before and after (acceptance 6).
///
/// "Before" places the polls of the same lowered module again with each
/// fact "each generator body" read as "none". An entry with no generator
/// function has no such fact, so the test lowers the entries whose source
/// has `function*` only, and the placement of each other entry is the same
/// on both sides by construction. A function with no resume can change:
/// a function that joins a cycle through a resume edge gains polls (`start`
/// and `step` of the shapes of the pass). The test does not show that the
/// edges of §351 are unchanged by a mask of another fact.
#[test]
fn masking_the_resume_fact_removes_polls_only_from_functions_in_a_cycle_through_a_resume() {
    let accept = corpus::corpus_accept();
    let mut before_total = 0usize;
    let mut after_total = 0usize;
    let mut changed = BTreeMap::new();
    let mut lowered = 0usize;
    for id in corpus::entry_ids(&accept) {
        let files = corpus::entry_sources(&accept, &id);
        if !files.iter().any(|file| file.source.contains("function*")) {
            continue;
        }
        let Ok(hir) = corpus::check_program(&files) else {
            continue;
        };
        let Ok(module) = lower_module(&hir) else {
            continue;
        };
        lowered += 1;
        let after = poll_sites(&module);
        let mut same = module.clone();
        place_collection_polls_with(&mut same, |fact| fact);
        assert_eq!(poll_sites(&same), after, "{id}: the identity mask");
        let mut masked = module.clone();
        place_collection_polls_with(&mut masked, |fact| match fact {
            ScriptEntry::GeneratorBody => ScriptEntry::None,
            other => other,
        });
        let before = poll_sites(&masked);
        before_total += before.iter().map(|(_, count)| count).sum::<usize>();
        after_total += after.iter().map(|(_, count)| count).sum::<usize>();
        let cycles = in_a_cycle_through_a_resume(&module);
        for (index, ((name, old), (_, new))) in before.iter().zip(&after).enumerate() {
            if old == new {
                continue;
            }
            assert!(
                cycles[index],
                "{id}: {name} is in no cycle through a resume edge, and its poll sites change \
                 from {old} to {new}"
            );
            *changed.entry(id.clone()).or_insert(0isize) += *new as isize - *old as isize;
        }
    }
    eprintln!(
        "entries with a generator {lowered}: poll sites {before_total} before, {after_total} after"
    );
    for (id, delta) in &changed {
        eprintln!("changed {id}: {delta:+}");
    }
    assert!(lowered > 0);
    assert!(after_total >= before_total);
}

/// The class, the bottom marker, and the exports of the shapes of the
/// descent through resume points (`compiler.md` §352 rule 9). `bottom`
/// collects at its first call, so the live set at that call is the set that
/// the program holds, and the count of requested bytes starts at zero.
/// `probe(depth, at)` stops on an index trap at call `at` of `bottom`.
const DOWN_COMMON: &str = r#"
class P { a: i64 = 0; b: i64 = 0; c: i64 = 0; d: i64 = 0; }
let stop: i32 = 0;
let reached: i32 = 0;
function bottom(): void {
  if (reached === 0) { Context.collect(); }
  markBottom();
  reached += 1;
  if (reached === stop) {
    const slot: FixedArray<i32, 1> = [0];
    print(`${slot[reached]}`);
  }
}
export function settle(): void { Context.collect(); }
export function run(depth: i32): void { reached = 0; descend(depth); }
export function probe(depth: i32, at: i32): void { stop = at; run(depth); }
export function main(): void { run(DEPTH); }
"#;

/// The objects that one level of a shape allocates on its descent: the
/// text `ALLOC` of a source.
const DOWN_OBJECTS: usize = 64;

/// The minimum headroom of the descent shapes: 96 KiB. The live set of
/// each shape at depth 256 is under it (the premise of [`down_bound`]).
const DOWN_HEADROOM: usize = 96 * 1024;

/// A shape of the descent: its name, its source, and the call of the
/// bottom marker that the dev JIT samples. The ship tier samples the last
/// call; the bound holds at each call.
struct DownShape {
    name: &'static str,
    source: &'static str,
    calls: i32,
}

/// The shapes of the descent. Each `next()` from the top runs one resume
/// of each level on the way down, and each level allocates `ALLOC` before
/// it resumes its child.
///
/// - `chain-straight`: each level resumes its child from straight code
///   after a `yield`. Only the poll at the resume point (§352 rule 9) is on
///   the way down. With the resume-point polls of `stage` removed, each
///   level keeps its `ALLOC` objects, so the count follows the depth and
///   passes the bound in each tier.
/// - `chain-loop`: the same in a `for` loop. The backedge poll of the loop
///   is on the path from the resume point, and it is the only poll there.
///   With the backedge polls of `stage` removed, the count follows the
///   depth and passes the bound.
/// - `walk-list`: the recursive walk of a tree, over a list of `depth + 1`
///   nodes. Each level allocates after its `yield x`, so the normal branch
///   of that resume point is open, and its poll is at its start; the
///   backedge poll of the inner `for…of` is after the allocation. Two polls
///   are thus on the way down, and each alone bounds it: a `for…of` has no
///   code between its backedge and its `next()`, so the allocation of the
///   way down is always between the two. The shape fails only with both
///   polls removed. The leaf of the list yields four times, so the walk
///   passes the same path four times. A level allocates only after the
///   first call of `bottom`: the walk of the list to its first leaf runs
///   `depth * depth / 2` resumes.
const DOWN_SHAPES: [DownShape; 3] = [
    DownShape {
        name: "chain-straight",
        calls: 4,
        source: r#"
function* stage(d: i32): Generator<i32> {
  if (d === 0) { bottom(); yield 0; bottom(); yield 0; bottom(); yield 0; bottom(); yield 0; return; }
  const child: Generator<i32> = stage(d - 1);
  child.next(); yield 1;
  ALLOC child.next(); yield 2;
  ALLOC child.next(); yield 3;
  ALLOC child.next(); yield 4;
}
function descend(d: i32): void {
  const top: Generator<i32> = stage(d);
  top.next(); top.next(); top.next(); top.next();
}
"#,
    },
    DownShape {
        name: "chain-loop",
        calls: 4,
        source: r#"
function* stage(d: i32): Generator<i32> {
  if (d === 0) { for (let i: i32 = 0; i < 4; i += 1) { bottom(); yield 0; } return; }
  const child: Generator<i32> = stage(d - 1);
  for (let i: i32 = 0; i < 4; i += 1) {
    ALLOC
    child.next();
    yield 1;
  }
}
function descend(d: i32): void {
  const top: Generator<i32> = stage(d);
  top.next(); top.next(); top.next(); top.next();
}
"#,
    },
    DownShape {
        name: "walk-list",
        calls: 4,
        source: r#"
class Node { value: i32 = 0; children: Node[] = []; }
function* walk(n: Node): Generator<i32> {
  if (n.children.length === 0) {
    bottom(); yield n.value; bottom(); yield n.value; bottom(); yield n.value; bottom(); yield n.value;
    return;
  }
  yield n.value;
  for (const c of n.children) {
    for (const x of walk(c)) { yield x; if (reached > 0) { ALLOC } }
  }
}
function tree(depth: i32): Node {
  let n: Node = new Node();
  for (let i: i32 = 0; i < depth; i += 1) {
    const p: Node = new Node();
    p.value = i;
    p.children.push(n);
    n = p;
  }
  return n;
}
function descend(d: i32): void {
  const root: Node = tree(d);
  for (const x of walk(root)) { }
}
"#,
    },
];

fn down_files(shape: &DownShape, depth: i32) -> [SourceFile; 2] {
    [
        SourceFile::ambient("return_path_marker.d.ts", MARKER),
        SourceFile::new(
            "generator-descent.ts",
            format!(
                "const DEPTH: i32 = {depth};\n{DOWN_COMMON}{}",
                shape
                    .source
                    .replace("ALLOC", &"new P(); ".repeat(DOWN_OBJECTS))
            ),
        ),
    ]
}

/// The bound of the objects that the program allocates after the first
/// call of `bottom` and that are live at the last call.
///
/// The first call collects, so the count of requested bytes is zero there.
/// Premise: the program holds at most `DOWN_HEADROOM` bytes at each later
/// collection, so the limit is the headroom (§346 rule 2). Let the last
/// collection before the last call be `C`. The objects that are live at
/// the last call and that the program allocated after the first call were
/// allocated after `C`. Their requested bytes are at most the count, plus
/// the bytes of the releases after `C` of blocks that were live at `C`
/// (§346 rule 2b), at most `DOWN_HEADROOM` by the premise. The count passes
/// the limit at one allocation; the level of that allocation runs to its
/// next poll with at most `per_frame` allocating instructions. Each
/// allocation of the descent requests `SIZE` bytes or more. Each call of
/// `bottom` is at the end of the same path, so a sample holds the frames of
/// the first call. So the objects are at most `2 * DOWN_HEADROOM / SIZE +
/// 1 + per_frame`. The bound does not depend on the depth.
///
/// The check needs a collection on the way down. In `chain-straight`
/// nothing allocates on the way up, and the sample is at the bottom, before
/// the way up. With no poll on the way down, the last descent leaves
/// `DOWN_OBJECTS` objects of each level live at the sample: 8,192 at depth
/// 128 and 16,384 at depth 256.
fn down_bound(shape: &DownShape) -> usize {
    let per_frame = per_frame_of(&down_files(shape, 0));
    let bound = 2 * DOWN_HEADROOM / SIZE + 1 + per_frame;
    // The check can fail: the descent of one pump at the smaller depth
    // allocates more than the bound.
    assert!(DOWN_OBJECTS * DEPTHS[0] as usize > bound, "{}", shape.name);
    bound
}

#[test]
fn a_descent_through_resume_points_reaches_a_poll_in_the_ship_tier() {
    let mut findings = Vec::new();
    for shape in &DOWN_SHAPES {
        let bound = down_bound(shape);
        let figures =
            return_path::ship_bottom_run(shape.name, &down_files(shape, 0), &DEPTHS, DOWN_HEADROOM);
        for figure in figures {
            let new = figure.last.saturating_sub(figure.first);
            eprintln!("ship {} bound={bound} new={new}: {figure:?}", shape.name);
            // The premise of the bound: the live bytes at the first call,
            // which follows a collection, are under the headroom.
            assert!(
                figure.first_bytes <= DOWN_HEADROOM as u64,
                "{}: {figure:?}",
                shape.name
            );
            assert!(figure.calls >= shape.calls as u64, "{}", shape.name);
            if new > bound as u64 {
                findings.push(format!(
                    "{} bound={bound} new={new}: {figure:?}",
                    shape.name
                ));
            }
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn a_descent_through_resume_points_reaches_a_poll_in_the_dev_jit() {
    let library = [dev_marker()];
    let mut findings = Vec::new();
    for shape in &DOWN_SHAPES {
        let bound = down_bound(shape);
        for depth in DEPTHS {
            let files = down_files(shape, depth);
            let first =
                return_path::dev_bottom(shape.name, &files, depth, 1, DOWN_HEADROOM, &library);
            let last = return_path::dev_bottom(
                shape.name,
                &files,
                depth,
                shape.calls,
                DOWN_HEADROOM,
                &library,
            );
            let new = last.saturating_sub(first);
            let figures = format!("depth={depth} first={first} last={last} new={new}");
            eprintln!("dev {} bound={bound}: {figures}", shape.name);
            if new > bound {
                findings.push(format!("{} bound={bound}: {figures}", shape.name));
            }
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

/// The resume points of the function whose first instruction is a poll,
/// and the count of its resume points.
fn polled_resume_points(function: &l::Function) -> (usize, usize) {
    let resumes = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            l::Terminator::Suspend {
                kind: l::SuspendKind::Yield(_),
                successor,
                ..
            } => Some(successor.0 as usize),
            _ => None,
        })
        .collect::<Vec<_>>();
    let polled = resumes
        .iter()
        .filter(|resume| {
            function.blocks[**resume]
                .instructions
                .first()
                .map(|first| &first.kind)
                == Some(&l::InstructionKind::CollectionPoll)
        })
        .count();
    (polled, resumes.len())
}

/// A chain of generators that each hold an object, a string, and an array
/// in locals across each `yield`, and read them after the poll at the
/// resume point (`compiler.md` §352 rule 9, §351 rule 10). Each pump from
/// the top resumes each level at a resume point. The depth is 6: the
/// interpreter runs each resume on the host stack.
const HELD: &str = r#"
class Q { x: i32 = 0; }
function* held(d: i32): Generator<string> {
  const q: Q = new Q();
  q.x = d;
  const s: string = `s${d}`;
  const a: i32[] = [d, d * 2];
  if (d === 0) {
    yield `${q.x}${s}${a[1]}`;
    yield `${q.x}${s}${a[1]}`;
    yield `${q.x}${s}${a[1]}`;
    return;
  }
  const child: Generator<string> = held(d - 1);
  yield `${q.x}${s}${a[1]}:${child.next().value}`;
  yield `${q.x}${s}${a[1]}:${child.next().value}`;
  yield `${q.x}${s}${a[1]}:${child.next().value}`;
}
export function main(): void {
  for (const v of held(6)) { print(v); }
}
"#;

#[test]
fn a_value_held_across_a_yield_is_rooted_at_the_resume_point_poll() {
    let files = [SourceFile::new("generator-held.ts", HELD)];
    let module = lower_module(&check_program(&files).expect("checks")).expect("lowers");
    let (polled, resumes) = polled_resume_points(function(&module, "held"));
    // The three resume points of the levels over the bottom read the held
    // values in a template, which allocates.
    assert!(polled >= 3, "{polled} of {resumes}");
    let (reference, _) = interpret_with_collection(&module, CollectionMode::Off, HEADROOM)
        .expect("the interpreter runs with mode 0");
    let reference = String::from_utf8(reference).expect("utf-8 output");
    assert_eq!(reference.lines().count(), 3, "{reference}");
    let (each_poll, run) = interpret_with_collection(&module, CollectionMode::EachPoll, HEADROOM)
        .expect("the interpreter runs with mode 2");
    assert!(run.automatic_collections > 0);
    assert_eq!(
        String::from_utf8(each_poll).expect("utf-8"),
        reference,
        "interpreter"
    );
    assert_eq!(
        dev_output(HELD, CollectionMode::Off),
        reference,
        "dev JIT mode 0"
    );
    assert_eq!(
        dev_output(HELD, CollectionMode::EachPoll),
        reference,
        "dev JIT mode 2"
    );
    assert_eq!(ship_output(HELD, 0), reference, "ship tier mode 0");
    assert_eq!(ship_output(HELD, 2), reference, "ship tier mode 2");
}
