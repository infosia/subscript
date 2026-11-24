//! Programs that store old objects into blocks during a mark
//! (`compiler.md` §356 rules 10, 13, and 20a).
//!
//! The ship tier runs each program in mode 3 with a budget of 4 words, so
//! each poll runs one pause, and the check of each pause reads each marked
//! block. With no barrier at a point that the program reaches, the check
//! stops the process. Measured with one point off at a time, each point
//! stops the programs that follow:
//!
//! - a field store: `a_field_store_of_an_old_object_during_a_mark`;
//! - the inline path of a push: `an_inline_push_of_an_old_object_during_a_mark`;
//! - a reserved push: `a_reserved_push_of_an_old_object_during_a_mark`;
//! - a store of a frame local: `a_traversal_subject_in_a_frame_during_a_mark`;
//! - the captures of a closure in a coroutine:
//!   `a_capture_of_a_closure_in_a_frame_during_a_mark` and
//!   `an_environment_copy_in_a_frame_during_a_mark`;
//! - the save at a suspension: `a_save_at_a_suspension_during_a_mark`,
//!   `a_fixed_array_saved_at_a_suspension_during_a_mark`,
//!   `a_parameter_of_a_new_coroutine_during_a_mark`, and
//!   `an_awaited_handle_in_a_frame_during_a_mark`;
//! - the awaited handle: `an_awaited_handle_in_a_frame_during_a_mark`;
//! - the coroutine entry: `a_parameter_of_a_new_coroutine_during_a_mark`,
//!   `an_awaited_handle_in_a_frame_during_a_mark`, and
//!   `an_awaited_call_of_an_old_object_during_a_mark`.
//!
//! The child of an `await` of a call stops no program: the child is
//! allocated with no pause before the store, so it has its mark (rule 4).
//! The copy of an environment stops no program alone: in these programs
//! the source environment is in the same frame, and the barrier of the
//! captures gave its words to the work list.
//!
//! Cost: one C compile and link and one run for each program; 2 s for the
//! eleven programs in a release build.

use std::path::{Path, PathBuf};

use subscript_codegen::{run_c_aot_configured_runs, NativeLibrary, RunConfig, RunError};
use subscript_compiler::SourceFile;

/// The pre-init hook that sets mode 3.
const SETUP_HOOK: &str = "subscript_barrier_points_setup";

/// The budget of one step, in words.
const BUDGET: u32 = 4;

/// The hook library that sets mode 3 with [`BUDGET`] before the
/// initializer.
fn setup_library(scratch: &Path) -> NativeLibrary {
    let source = scratch.join("barrier_points_setup.c");
    std::fs::write(
        &source,
        format!(
            "#include <stdint.h>\n\
             #include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             void {SETUP_HOOK}(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection_step(ctx, {BUDGET}u) != 0\n\
                   || subscript_rt_ctx_set_collection(ctx, 3, 0u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n"
        ),
    )
    .expect("write the setup hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only the pre-init hook; no script calls it.
    unsafe { NativeLibrary::new(vec![repo.join("runtime/include")], vec![source], Vec::new()) }
}

/// Runs `source` in the ship tier in mode 3. Returns the stdout, or the
/// failure with the stderr of the process.
fn run_mode_three(name: &str, source: &str) -> Result<String, String> {
    let scratch = std::env::temp_dir().join(format!(
        "subscript-barrier-points-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create the scratch directory");
    let libraries = [setup_library(&scratch)];
    let mut config = RunConfig::default().with_native_libraries(&libraries);
    config.pre_init_hook = Some(SETUP_HOOK);
    let files = [SourceFile::new("main.ts", source)];
    let result = run_c_aot_configured_runs(&files, config, &[&[]]);
    let _ = std::fs::remove_dir_all(&scratch);
    let run = result
        .map_err(|error| format!("build: {error}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "no run".to_string())?;
    match run {
        Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        Err(RunError::AbnormalTermination(termination)) => Err(format!(
            "{}: {}",
            termination.status,
            String::from_utf8_lossy(&termination.stderr)
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// Asserts that `source` runs to the end in mode 3 and prints `expected`.
fn assert_runs(name: &str, source: &str, expected: &str) {
    match run_mode_three(name, source) {
        Ok(stdout) => assert_eq!(stdout, expected, "{name}"),
        Err(failure) => panic!("{name}: {failure}"),
    }
}

/// The old objects of each program: `count` pairs in an array, made before
/// the stores. Each pair holds the only reference to its node.
///
/// A block that is allocated during a cycle has its mark (rule 4), so an
/// object is old only for a cycle that starts after its allocation. A
/// cycle of mode 3 has at most 8 steps (rule 19), so the cycles that start
/// after the pairs are made find them old.
///
/// A step reads four words of the array storage at a time, from the first
/// element, and pops no pair until that scan ends. The work list then
/// gives the last pair first. So the programs read the pairs from the
/// first: the node of a pair that the mark did not reach has no mark and
/// is not in the work list.
const OLDS: &str = r#"
class Node { next: Node | null = null; value: i32 = 0; }
class Pair { other: Node; constructor(other: Node) { this.other = other; } }
function olds(count: i32): Pair[] {
  const pairs: Pair[] = [];
  for (let i: i32 = 0; i < count; i += 1) { const node: Node = new Node(); node.value = i; pairs.push(new Pair(node)); }
  return pairs;
}
function total(nodes: Node[]): i32 {
  let sum: i32 = 0;
  for (const node of nodes) { const next: Node | null = node.next; if (next !== null) { sum += next.value; } else { sum += node.value; } }
  return sum;
}
"#;

/// `ArrayPushReserved` in a static `map` (`compiler.md` §342 rule 10b): the
/// element goes into the result storage, which the allocation marked.
#[test]
fn a_reserved_push_of_an_old_object_during_a_mark() {
    let source = format!(
        "{OLDS}
export function main(): void {{
  const nodes: Pair[] = olds(3000);
  const out: Node[] = nodes.map((pair: Pair): Node => pair.other);
  print(`${{total(out)}}`);
}}"
    );
    assert_runs("reserved", &source, "4498500\n");
}

/// An async function that suspends once.
const ZERO: &str = r#"
async function zero(): Promise<i32> { await Context.suspend(); return 0; }
"#;

/// A fixed array that a call gives, indexed by an `await` (`compiler.md`
/// §356 rule 13): the value is live across the suspension, and the save
/// puts it into the frame.
#[test]
fn a_fixed_array_saved_at_a_suspension_during_a_mark() {
    let source = format!(
        "{OLDS}{ZERO}
function pairOf(node: Node): FixedArray<Node, 2> {{ return [node, node]; }}
async function walk(nodes: Pair[]): Promise<i32> {{
  let sum: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{ sum += pairOf(nodes[i].other)[await zero()].value; }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("fixed", &source, "4498500\n");
}

/// A function value made in a coroutine from a closure of the same frame
/// (`compiler.md` §356 rule 10): its environment is copied into the frame.
#[test]
fn an_environment_copy_in_a_frame_during_a_mark() {
    let source = format!(
        "{OLDS}{ZERO}
function apply(read: () => i32): i32 {{ return read(); }}
async function walk(nodes: Pair[]): Promise<i32> {{
  let sum: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{
    const node: Node = nodes[i].other;
    const read = (): i32 => node.value;
    const again: () => i32 = read;
    await zero();
    sum += apply(again);
  }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("environment", &source, "4498500\n");
}

/// The async helpers of the frame programs.
const TAKE: &str = r#"
async function take(node: Node): Promise<i32> { await Context.suspend(); return node.value; }
"#;

/// The save of a value at a suspension (`compiler.md` §356 rule 13): the
/// old node is live across the `await`, so it goes into the frame.
#[test]
fn a_save_at_a_suspension_during_a_mark() {
    let source = format!(
        "{OLDS}{ZERO}
async function walk(nodes: Pair[]): Promise<i32> {{
  let sum: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{ const node: Node = nodes[i].other; await zero(); sum += node.value; }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("save", &source, "4498500\n");
}

/// The entry of a coroutine (`compiler.md` §356 rule 13): the parameter
/// goes into the new frame, which the allocation marked.
#[test]
fn a_parameter_of_a_new_coroutine_during_a_mark() {
    let source = format!(
        "{OLDS}{TAKE}
async function walk(nodes: Pair[]): Promise<i32> {{
  const handles: Promise<i32>[] = [];
  for (let i: i32 = 0; i < 3000; i += 1) {{ handles.push(take(nodes[i].other)); }}
  let sum: i32 = 0;
  for (const handle of handles) {{ sum += await handle; }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("entry", &source, "4498500\n");
}

/// The awaited handle of an `await` of a handle (`compiler.md` §356 rule
/// 13): the handle goes into the frame. The handles are made before, and
/// the program awaits them in reverse order.
#[test]
fn an_awaited_handle_in_a_frame_during_a_mark() {
    let source = format!(
        "{OLDS}{TAKE}
async function walk(nodes: Pair[]): Promise<i32> {{
  const handles: Promise<i32>[] = [];
  for (const pair of nodes) {{ handles.push(take(pair.other)); }}
  let sum: i32 = 0;
  for (let i: i32 = 2999; i >= 0; i -= 1) {{ sum += await handles[i]; }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("handle", &source, "4498500\n");
}

/// A closure in a coroutine (`compiler.md` §356 rule 13): its captures are
/// in the frame.
#[test]
fn a_capture_of_a_closure_in_a_frame_during_a_mark() {
    let source = format!(
        "{OLDS}{ZERO}
async function walk(nodes: Pair[]): Promise<i32> {{
  let sum: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{
    const node: Node = nodes[i].other;
    const read = (): i32 => node.value;
    sum += read();
    await zero();
  }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("capture", &source, "4498500\n");
}

/// A field store in a synchronous function (`compiler.md` §356 rule 13):
/// the new holder has its mark, and the old node that it gets has none.
#[test]
fn a_field_store_of_an_old_object_during_a_mark() {
    let source = format!(
        "{OLDS}
class Holder {{ node: Node | null = null; }}
export function main(): void {{
  const pairs: Pair[] = olds(3000);
  const holders: Holder[] = [];
  for (let i: i32 = 0; i < 3000; i += 1) {{ const holder: Holder = new Holder(); holder.node = pairs[i].other; holders.push(holder); }}
  let sum: i32 = 0;
  for (const holder of holders) {{ const node: Node | null = holder.node; if (node !== null) {{ sum += node.value; }} }}
  print(`${{sum}}`);
}}"
    );
    assert_runs("field", &source, "4498500\n");
}

/// The inline path of a push in a synchronous function (`compiler.md`
/// §356 rule 13): the storage of the new array has its mark, and the old
/// node that goes into it has none.
#[test]
fn an_inline_push_of_an_old_object_during_a_mark() {
    let source = format!(
        "{OLDS}
export function main(): void {{
  const pairs: Pair[] = olds(3000);
  const out: Node[] = [];
  let ticks: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{ out.push(pairs[i].other); const tick: Node = new Node(); ticks += tick.value; }}
  print(`${{total(out) + ticks}}`);
}}"
    );
    assert_runs("push", &source, "4498500\n");
}

/// The child of an `await` of a call (`compiler.md` §356 rule 13): the
/// handle of the new child goes into the frame, and the parameter goes
/// into the frame of the child.
#[test]
fn an_awaited_call_of_an_old_object_during_a_mark() {
    let source = format!(
        "{OLDS}{TAKE}
async function walk(nodes: Pair[]): Promise<i32> {{
  let sum: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{ sum += await take(nodes[i].other); }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("child", &source, "4498500\n");
}

/// The hidden local of a traversal of a fixed array in a coroutine
/// (`compiler.md` §356 rule 13): the subject is live across the `await`, so
/// it is stored in the frame. The first element is read before the `await`.
#[test]
fn a_traversal_subject_in_a_frame_during_a_mark() {
    let source = format!(
        "{OLDS}{ZERO}
function pairOf(node: Node): FixedArray<Node, 2> {{ return [node, node]; }}
async function walk(nodes: Pair[]): Promise<i32> {{
  let sum: i32 = 0;
  for (let i: i32 = 0; i < 3000; i += 1) {{
    let first: boolean = true;
    for (const node of pairOf(nodes[i].other)) {{ if (first) {{ sum += node.value; }} first = false; await zero(); }}
  }}
  return sum;
}}
export async function main(): Promise<void> {{
  print(`${{await walk(olds(3000))}}`);
}}"
    );
    assert_runs("subject", &source, "4498500\n");
}
