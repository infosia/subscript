//! The adversarial list of the sandbox profile
//! (`specs/blocks/compiler.md` §109.8 criterion 4).
//!
//! Five shapes. Each one is built here, not read from the corpus, and each
//! one goes through the checker under the profile. A shape that checks
//! clean then runs on both tiers under the profile.
//!
//! | Shape | Must |
//! |---|---|
//! | a 131,073-byte source | reject S026 |
//! | a 257-deep parenthesis source | reject S026 |
//! | recursion with no base case | trap `stack-budget` on both tiers |
//! | an allocation loop that keeps every block live | trap `allocation-quota` on both tiers |
//! | `Context.fromBytes` of forged bytes | reject S024 |
//!
//! Every shape carries a control. A reject shape's control is the same
//! source under the default profile, which must check clean. A trap
//! shape's control is the same program with a base case or a bounded
//! loop, which must run clean under the profile.
//!
//! More shapes follow the list. The last one measures the print sink
//! under the profile (§109.7a), so a counting global allocator records
//! the peak of this test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use subscript_codegen::{run_c_aot_configured, run_jit_configured, RunConfig, RunError};
use subscript_compiler::{
    check_program_with, on_the_compile_thread, CheckOptions, Profile, RuleCode, SourceFile,
};
use subscript_runtime::TrapKind;

/// Live bytes the process holds, as the allocator sees them.
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// The largest value [`LIVE`] reached since the window opened.
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// The smallest value [`LIVE`] reached since the window opened.
///
/// The counters are process-wide and the other tests of this file run
/// beside the measurement, so a window reads `PEAK - FLOOR`: the largest
/// excursion above the lowest live value it saw. `PEAK - opened`
/// under-counts by whatever another thread frees inside the window.
static FLOOR: AtomicUsize = AtomicUsize::new(0);

/// The system allocator with a live-bytes counter and a peak record.
struct Counting;

// SAFETY: every method forwards to the system allocator with the same
// pointer and layout it was given; the counters add no requirement.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            PEAK.fetch_max(
                LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size(),
                Ordering::Relaxed,
            );
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        FLOOR.fetch_min(
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed) - layout.size(),
            Ordering::Relaxed,
        );
        // SAFETY: forwarded caller contract.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let fresh = unsafe { System.realloc(pointer, layout, new_size) };
        if !fresh.is_null() {
            PEAK.fetch_max(
                LIVE.fetch_add(new_size, Ordering::Relaxed) + new_size,
                Ordering::Relaxed,
            );
            FLOOR.fetch_min(
                LIVE.fetch_sub(layout.size(), Ordering::Relaxed) - layout.size(),
                Ordering::Relaxed,
            );
        }
        fresh
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            PEAK.fetch_max(
                LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size(),
                Ordering::Relaxed,
            );
        }
        pointer
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Opens a measurement window at the current live bytes.
fn open_window() {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.store(live, Ordering::Relaxed);
    FLOOR.store(live, Ordering::Relaxed);
}

/// The bytes the window held at once: the peak above its own floor.
fn close_window() -> usize {
    PEAK.load(Ordering::Relaxed)
        .saturating_sub(FLOOR.load(Ordering::Relaxed))
}

/// §109.2 S026: the byte limit of one source file.
const SOURCE_BYTE_LIMIT: usize = 131_072;

/// §109.2 rule 2: the nesting limit of the checker's descent.
const NESTING_DEPTH_LIMIT: usize = 256;

fn files(source: String) -> Vec<SourceFile> {
    vec![SourceFile::new("adversarial.ts", source)]
}

/// Checks `source` under `profile` and returns the first diagnostic code.
///
/// §109.2 rule 3: the whole compile runs on the compile thread, so an
/// adversarial depth is bounded by that thread and not by this one.
fn check_under(profile: Profile, source: &str) -> Result<(), RuleCode> {
    let options = CheckOptions::with_profile(profile);
    on_the_compile_thread(
        || match check_program_with(&files(source.to_string()), &options) {
            Ok(_) => Ok(()),
            Err(diagnostics) => Err(diagnostics
                .first()
                .expect("a rejection carries a diagnostic")
                .code),
        },
    )
}

/// Asserts that the profile rejects `source` with `code`, and that the
/// default profile checks the same source clean.
fn assert_profile_rejects(shape: &str, source: &str, code: RuleCode) {
    match check_under(Profile::Sandbox, source) {
        Err(reported) => assert_eq!(reported, code, "{shape}: the profile reported {reported:?}"),
        Ok(()) => panic!("{shape}: the profile accepted the source"),
    }
    // The firing control: the same bytes under the default profile.
    match check_under(Profile::Default, source) {
        Ok(()) => {}
        Err(reported) => panic!("{shape}: the control was rejected with {reported:?}"),
    }
    println!("{shape}: rejected {code:?}; the default-profile control checks clean");
}

/// Asserts that both tiers trap with `kind` when they run `source` under
/// the profile, and returns the pre-trap stdout each tier produced.
fn assert_both_tiers_trap(shape: &str, source: &str, kind: TrapKind) {
    let config = RunConfig::with_profile(Profile::Sandbox);
    assert_eq!(
        check_under(Profile::Sandbox, source),
        Ok(()),
        "{shape}: the profile must accept the source before a tier runs it"
    );
    for (tier, outcome) in [
        (
            "dev-JIT",
            run_jit_configured(&files(source.to_string()), config).map(|run| run.stdout),
        ),
        (
            "ship-C-AOT",
            run_c_aot_configured(&files(source.to_string()), config).map(|run| run.stdout),
        ),
    ] {
        match outcome {
            Err(RunError::Trap(report)) => {
                assert_eq!(report.rule, kind, "{shape} on {tier}");
                println!(
                    "{shape} on {tier}: trap `{}` at {}:{}",
                    report.rule.rule(),
                    report.pos.line,
                    report.pos.col
                );
            }
            Ok(stdout) => panic!(
                "{shape} on {tier}: the run completed with {:?}",
                String::from_utf8_lossy(&stdout)
            ),
            Err(error) => panic!(
                "{shape} on {tier}: expected trap `{}`, got {error}",
                kind.rule()
            ),
        }
    }
}

/// The firing control of a trap shape: the bounded program runs clean on
/// both tiers under the profile and prints `expected`.
fn assert_both_tiers_run(shape: &str, source: &str, expected: &[u8]) {
    let config = RunConfig::with_profile(Profile::Sandbox);
    for (tier, outcome) in [
        (
            "dev-JIT",
            run_jit_configured(&files(source.to_string()), config).map(|run| run.stdout),
        ),
        (
            "ship-C-AOT",
            run_c_aot_configured(&files(source.to_string()), config).map(|run| run.stdout),
        ),
    ] {
        match outcome {
            Ok(stdout) => assert_eq!(stdout, expected, "{shape} control on {tier}"),
            Err(error) => panic!("{shape} control on {tier}: {error}"),
        }
    }
    println!("{shape}: the bounded control runs clean on both tiers");
}

/// A program of exactly `bytes` bytes: one entry, then a line comment
/// padded to the length.
fn source_of_length(bytes: usize) -> String {
    const BODY: &str = "export function main(): void {\n  print(\"pad\");\n}\n// ";
    assert!(bytes > BODY.len(), "the padded source must hold the body");
    let mut source = String::with_capacity(bytes);
    source.push_str(BODY);
    source.extend(std::iter::repeat_n('x', bytes - BODY.len() - 1));
    source.push('\n');
    assert_eq!(source.len(), bytes);
    source
}

/// A program whose nesting depth reaches `depth`: one parenthesized
/// integer inside the entry's body, whose declaration is the first
/// level.
fn source_of_depth(depth: usize) -> String {
    let inner = depth - 1;
    format!(
        "export function main(): void {{\n  const value: i32 = {}7{};\n  print(`${{value}}`);\n}}\n",
        "(".repeat(inner),
        ")".repeat(inner),
    )
}

#[test]
fn a_source_one_byte_over_the_limit_rejects_s026() {
    let source = source_of_length(SOURCE_BYTE_LIMIT + 1);
    assert_profile_rejects("131,073-byte source", &source, RuleCode::S026);
    // The limit itself is accepted, so the rejection is the extra byte.
    let at_the_limit = source_of_length(SOURCE_BYTE_LIMIT);
    assert_eq!(
        check_under(Profile::Sandbox, &at_the_limit),
        Ok(()),
        "the profile rejected a source of exactly the limit"
    );
    println!("131,072-byte source: the profile accepts it");
}

/// §109.2 rule 2: the nesting guard is the one depth bound, so a
/// parenthesis nest over it reports after the parse.
#[test]
fn a_source_one_parenthesis_over_the_limit_rejects_s026() {
    let source = source_of_depth(NESTING_DEPTH_LIMIT + 1);
    assert_profile_rejects("257-deep parenthesis source", &source, RuleCode::S026);
    // A parenthesis is an expression node, and the declaration statement
    // and the literal are nodes of their own, so 254 parentheses reach
    // the limit. 255 is the deepest parenthesis source the profile
    // accepts.
    let at_the_limit = source_of_depth(NESTING_DEPTH_LIMIT - 1);
    assert_eq!(
        check_under(Profile::Sandbox, &at_the_limit),
        Ok(()),
        "the profile rejected the deepest accepted parenthesis source"
    );
    assert_eq!(
        check_under(Profile::Sandbox, &source_of_depth(NESTING_DEPTH_LIMIT)),
        Err(RuleCode::S026),
        "one level more must report"
    );
    println!("255-deep parenthesis source: the profile accepts it; 256 reports");
}

#[test]
fn recursion_with_no_base_case_traps_the_stack_budget_on_both_tiers() {
    const ENDLESS: &str = "\
function descend(depth: i32): i32 {\n\
\x20 return descend(depth + 1) + 1;\n\
}\n\
export function main(): void {\n\
\x20 print(\"start\");\n\
\x20 print(`${descend(0)}`);\n\
}\n";
    const BOUNDED: &str = "\
function descend(depth: i32): i32 {\n\
\x20 if (depth > 16) {\n\
\x20   return depth;\n\
\x20 }\n\
\x20 return descend(depth + 1) + 1;\n\
}\n\
export function main(): void {\n\
\x20 print(\"start\");\n\
\x20 print(`${descend(0)}`);\n\
}\n";
    assert_both_tiers_trap(
        "recursion with no base case",
        ENDLESS,
        TrapKind::StackBudget,
    );
    assert_both_tiers_run("recursion with no base case", BOUNDED, b"start\n34\n");
}

#[test]
fn an_allocation_loop_traps_the_quota_on_both_tiers() {
    // Each block is kept in `blocks`, so nothing leaves the live set and
    // the 64 MiB default quota is reached. 300 blocks of 256 KiB is 75 MiB.
    const UNBOUNDED: &str = "\
export function main(): void {\n\
\x20 print(\"start\");\n\
\x20 let seed: u8[] = [1];\n\
\x20 for (let step: i32 = 0; step < 18; step = step + 1) {\n\
\x20   seed = seed.concat(seed);\n\
\x20 }\n\
\x20 const blocks: u8[][] = [];\n\
\x20 for (let block: i32 = 0; block < 300; block = block + 1) {\n\
\x20   blocks.push(seed.slice(0, seed.length));\n\
\x20 }\n\
\x20 print(`${blocks.length}`);\n\
}\n";
    // The same shape with a bounded count: 16 blocks is 4 MiB.
    const BOUNDED: &str = "\
export function main(): void {\n\
\x20 print(\"start\");\n\
\x20 let seed: u8[] = [1];\n\
\x20 for (let step: i32 = 0; step < 18; step = step + 1) {\n\
\x20   seed = seed.concat(seed);\n\
\x20 }\n\
\x20 const blocks: u8[][] = [];\n\
\x20 for (let block: i32 = 0; block < 16; block = block + 1) {\n\
\x20   blocks.push(seed.slice(0, seed.length));\n\
\x20 }\n\
\x20 print(`${blocks.length}`);\n\
}\n";
    assert_both_tiers_trap(
        "allocation loop with every block live",
        UNBOUNDED,
        TrapKind::AllocationQuota,
    );
    assert_both_tiers_run(
        "allocation loop with every block live",
        BOUNDED,
        b"start\n16\n",
    );
}

#[test]
fn from_bytes_of_forged_bytes_rejects_s024() {
    // The bytes are the caller's, so the layout they claim is unproven.
    // The profile rejects the call, whatever the bytes hold.
    const FORGED: &str = "\
@CStruct({ align: 4 })\n\
class Point {\n\
\x20 x: i32 = 0;\n\
\x20 y: i32 = 0;\n\
}\n\
export function main(): void {\n\
\x20 const bytes: u8[] = [255, 255, 255, 127, 1, 0, 0, 0];\n\
\x20 const point: Point = Context.fromBytes<Point>(bytes, 0);\n\
\x20 print(`${point.x},${point.y}`);\n\
}\n";
    assert_profile_rejects("Context.fromBytes of forged bytes", FORGED, RuleCode::S024);
}

/// §109.2 rule 2: the four shapes the amendment names. The nesting guard
/// is the one depth bound, so every one of them reports through it.
///
/// Each shape is far past the limit, so the guard stops the descent long
/// before any later stage walks the tree.
fn nesting_shapes(count: usize) -> [(&'static str, String); 4] {
    let mut arrows = String::from("1");
    for _ in 0..count {
        arrows = format!("((): i32 => {arrows})()");
    }
    [
        (
            "nested type arguments",
            format!(
                "export function main(): void {{\n  const deep: {}i32{} = [];\n  print(`${{deep.length}}`);\n}}\n",
                "Array<".repeat(count),
                ">".repeat(count)
            ),
        ),
        (
            "prefix `!` chain",
            format!(
                "export function main(): void {{\n  const flag: boolean = {}true;\n  print(`${{flag}}`);\n}}\n",
                "!".repeat(count)
            ),
        ),
        (
            "conditional `? :` chain",
            format!(
                "export function main(): void {{\n  const value: i32 = {}1;\n  print(`${{value}}`);\n}}\n",
                "true ? 1 : ".repeat(count)
            ),
        ),
        (
            "nested arrow bodies",
            format!(
                "export function main(): void {{\n  const value: i32 = {arrows};\n  print(`${{value}}`);\n}}\n"
            ),
        ),
    ]
}

#[test]
fn every_nesting_shape_over_the_limit_rejects_s026() {
    for (shape, source) in nesting_shapes(1_000) {
        assert_profile_rejects(shape, &source, RuleCode::S026);
    }
}

#[test]
fn every_nesting_shape_at_the_limit_is_accepted() {
    // The declaration statement and the leaf are levels of their own, and
    // one arrow level is two, so these counts are the measured deepest
    // form of each shape.
    for (deepest, index) in [(254, 0), (254, 1), (254, 2), (127, 3)] {
        let (shape, source) = &nesting_shapes(deepest)[index];
        assert_eq!(
            check_under(Profile::Sandbox, source),
            Ok(()),
            "{shape}: the profile rejected its deepest accepted form"
        );
        // The firing control: one level more reports.
        let (_, over) = &nesting_shapes(deepest + 1)[index];
        assert_eq!(
            check_under(Profile::Sandbox, over),
            Err(RuleCode::S026),
            "{shape}: one level over the limit must report"
        );
        println!(
            "{shape}: {deepest} levels accepted, {} rejected",
            deepest + 1
        );
    }
}

/// §109.2 S027: the profile lowers the frame limit to 65,536 bytes.
#[test]
fn a_frame_over_the_profile_limit_rejects_s027() {
    let frame = |bytes: usize| {
        format!(
            "function probe(input: FixedArray<u8, {bytes}>): u8 {{\n  const block: FixedArray<u8, {bytes}> = input;\n  return block[0];\n}}\n\nexport function main(): void {{\n  print(\"frame\");\n}}\n"
        )
    };
    assert_profile_rejects("a 65,537-byte frame", &frame(65_537), RuleCode::S027);
    assert_eq!(
        check_under(Profile::Sandbox, &frame(65_536)),
        Ok(()),
        "the profile rejected a frame of exactly the limit"
    );
    println!("65,536-byte frame: the profile accepts it");
}

/// §109.2 S026: the program limit is the byte sum over every file the
/// entry imports.
#[test]
fn a_program_over_the_byte_limit_rejects_s026() {
    const FIVE_MEBIBYTES: usize = 5 * 1024 * 1024;
    let sources = vec![
        SourceFile::new(
            "adversarial.ts",
            format!(
                "export function main(): void {{\n  print(\"program\");\n}}\n// {}\n",
                "x".repeat(FIVE_MEBIBYTES)
            ),
        ),
        SourceFile::new("other.ts", format!("// {}\n", "y".repeat(FIVE_MEBIBYTES))),
    ];
    let options = CheckOptions::with_profile(Profile::Sandbox);
    let diagnostics = on_the_compile_thread(|| {
        check_program_with(&sources, &options)
            .err()
            .unwrap_or_default()
    });
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S026);
    assert_eq!(diagnostics[0].pos.file, "adversarial.ts");
    println!(
        "two 5 MiB files: {} at {}",
        diagnostics[0].message, diagnostics[0].pos
    );
}

/// §109.2 rule 3: the compile of a dev-tier run is on the compile
/// thread, so a depth no caller thread holds still runs. The thread here
/// is 2 MiB, the size of an ordinary test thread, and the source nests
/// 2,000 parentheses.
#[test]
fn a_deep_source_runs_through_the_dev_jit_from_a_two_mebibyte_thread() {
    let source = format!(
        "export function main(): void {{\n  const value: i32 = {}7{};\n  print(`${{value}}`);\n}}\n",
        "(".repeat(2_000),
        ")".repeat(2_000)
    );
    let ran = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            run_jit_configured(&files(source), RunConfig::default()).map(|run| run.stdout)
        })
        .expect("spawn the 2 MiB caller thread")
        .join()
        .expect("the run returns from a 2 MiB caller thread");
    assert_eq!(ran.expect("the deep source runs"), b"7\n");
    println!("2,000 nested parentheses: the dev JIT runs it from a 2 MiB caller thread");
}

/// A regular-expression literal that holds a backtick, then a nest of
/// `levels` type arguments.
///
/// §109.2 rule 5: the S026 byte check counts bytes and reads no token,
/// so it is exact on a source a lexer alone cannot read. A lexer with
/// no parser takes the `/` for division and the backtick for a template
/// head, and desyncs on every later token of this file.
fn regex_then_type_nest(levels: usize) -> String {
    format!(
        "export function main(): void {{\n  const r: RegExp = /`/;\n  const deep: {}i32{} = [];\n  print(`${{r.source.length}}${{deep.length}}`);\n}}\n",
        "Array<".repeat(levels),
        ">".repeat(levels)
    )
}

/// §109.2 rule 5: over the byte limit the byte check reports and the
/// parser never runs. Inside the limit the parser runs the whole nest
/// and the nesting guard of rule 2 reports.
#[test]
fn a_regex_literal_before_a_deep_type_nest_rejects_s026() {
    // 300,000 levels is 2,100,000 bytes, far over the byte limit. The
    // default profile keeps no limit and would parse every level, so
    // this shape carries no default-profile control; the shape below is
    // the control that the limit is the profile's.
    let over = regex_then_type_nest(300_000);
    assert!(over.len() > SOURCE_BYTE_LIMIT, "{} bytes", over.len());
    assert_eq!(
        check_under(Profile::Sandbox, &over),
        Err(RuleCode::S026),
        "the byte limit must report before the parser"
    );
    println!("regex literal before 300,000 type levels: S026 before the parser");

    // The same shape inside the byte limit: the parser runs it and the
    // nesting guard reports after the parse.
    let under = regex_then_type_nest(1_000);
    assert!(under.len() <= SOURCE_BYTE_LIMIT, "{} bytes", under.len());
    assert_profile_rejects(
        "regex literal before 1,000 type levels",
        &under,
        RuleCode::S026,
    );
}

/// §109.2 rule 5: a quote inside a regular-expression literal, on 257
/// lines.
///
/// The S026 check counts bytes and reads no token, so a quote a lexer
/// alone takes for the start of a string literal costs this source
/// nothing: it checks clean under the profile and runs on both tiers.
#[test]
fn a_quote_inside_a_regex_literal_checks_clean_and_runs() {
    let mut source =
        String::from("export function main(): void {\n  let s: string = \"a\\\"b\";\n");
    for _ in 0..257 {
        source.push_str("  s = s.replace(/\"/g, \"0\");\n");
    }
    source.push_str("  print(s);\n}\n");
    assert_eq!(
        check_under(Profile::Sandbox, &source),
        Ok(()),
        "257 quoted regex literals must check clean under the profile"
    );
    assert_both_tiers_run("quote inside a regex literal", &source, b"a0b\n");
}

/// A chain of `labels` same labels, inside S026's byte limit.
///
/// The parser's duplicate-label path is superlinear in memory on this
/// shape (M13).
fn same_label_chain(labels: usize) -> String {
    format!(
        "export function main(): void {{}}\n{};\n",
        "a:".repeat(labels)
    )
}

/// A generic call whose type argument nests `levels` deep over a leaf
/// that is an expression and not a type.
///
/// The parser reads the whole nest before the leaf refuses it, so its
/// work is superlinear in the level count (M13).
fn nested_generic_call(levels: usize) -> String {
    format!(
        "export function main(): void {{\n  const x: i32 = f<{}1+1{}>(1);\n  print(`${{x}}`);\n}}\n",
        "A<".repeat(levels),
        ">".repeat(levels)
    )
}

/// §109.2 rule 6: the two shapes of M13 are the parser's, and no rule
/// of §109.2 rejects either one.
///
/// At the sizes here each shape returns in milliseconds with the
/// parser's own S100, under both profiles. The profile therefore adds
/// no rule for them, and what bounds them at S026's byte limit is the
/// budgeted child: there the label chain passes the memory budget and
/// the generic call passes the time budget, each as one S026.
#[test]
fn the_parser_shapes_of_m13_carry_no_profile_rule() {
    for (shape, source) in [
        ("same-label chain, 2,000 labels", same_label_chain(2_000)),
        ("nested generic call, 500 levels", nested_generic_call(500)),
    ] {
        for profile in [Profile::Sandbox, Profile::Default] {
            assert_eq!(
                check_under(profile, &source),
                Err(RuleCode::S100),
                "{shape} under {profile:?}"
            );
        }
        println!("{shape}: the parser reports S100 under both profiles");
    }
}
/// A 65,536-byte line printed 4,096 times.
///
/// The program prints 256 MiB, four times the profile's default quota.
fn sink_source() -> String {
    concat!(
        "export function main(): void {\n",
        "  const line: string = \"x\".repeat(65536);\n",
        "  for (let step: i32 = 0; step < 4096; step = step + 1) {\n",
        "    print(line);\n",
        "  }\n",
        "}\n",
    )
    .to_string()
}

/// §109.7a: the dev-JIT runner installs no print observer under the
/// profile, so the printed bytes live in the Context sink, the quota
/// charges them, and the run traps instead of holding 256 MiB of host
/// memory.
///
/// The measurement is in process: `memory_accounting` keeps the run on
/// this thread, so the counting allocator above sees the run's bytes.
/// The control is the same program under the default profile, whose
/// observer buffer holds every line; it is also the firing control of
/// the measurement, because it is the peak the profile must not reach.
#[test]
fn the_print_sink_bounds_the_dev_jit_peak_under_the_profile() {
    /// The bytes of one line, without its newline.
    const LINE: usize = 65_536;
    /// The lines the program prints.
    const LINES: usize = 4_096;
    /// §109.5: the profile's default allocation quota.
    const QUOTA: usize = 67_108_864;
    /// The bytes the run may hold above the quota.
    ///
    /// The sink is a `Vec`, which doubles its buffer, so the copy that
    /// reaches the quota holds the old buffer and the new one at once:
    /// the measured peak is 100,870,977 bytes, 33.8 MB over the quota.
    /// The other tests of this file allocate beside the window, so the
    /// bound holds 64 MiB over the quota.
    const SLACK: usize = 64 * 1024 * 1024;

    // `memory_accounting` keeps the run in this process, where the
    // counting allocator sees it.
    let config = |profile| {
        let mut config = RunConfig::with_profile(profile);
        config.memory_accounting = true;
        config
    };

    open_window();
    let stopped = run_jit_configured(&files(sink_source()), config(Profile::Sandbox));
    let peak = close_window();
    let stdout = match stopped {
        Err(RunError::Trap(report)) => {
            assert_eq!(report.rule, TrapKind::AllocationQuota, "{}", report.message);
            report.stdout
        }
        // A run that does not trap returns every printed byte, so the
        // report names its length and not its bytes.
        Ok(output) => panic!(
            "the profile must trap the print sink: it ran and printed {} bytes",
            output.stdout.len()
        ),
        Err(other) => panic!("the profile must trap the print sink: {other}"),
    };
    // The bytes before the trap are whole lines, and each one is intact.
    assert_eq!(stdout.len() % (LINE + 1), 0, "{} bytes", stdout.len());
    let lines = stdout.len() / (LINE + 1);
    assert!(lines > 0 && lines < LINES, "{lines} lines before the trap");
    let one = [b"x".repeat(LINE), b"\n".to_vec()].concat();
    assert!(
        stdout.chunks_exact(LINE + 1).all(|chunk| chunk == one),
        "a line before the trap is not intact"
    );
    println!(
        "sink.ts under the profile: {lines} lines, {} bytes, peak {peak} bytes",
        stdout.len()
    );
    assert!(
        peak < QUOTA + SLACK,
        "the profile run peaked at {peak} bytes over a {QUOTA}-byte quota"
    );

    // The control: no quota, so the run keeps all 4,096 lines and the
    // same measurement reads a peak the profile never reaches.
    open_window();
    let completed = run_jit_configured(&files(sink_source()), config(Profile::Default));
    let control_peak = close_window();
    let all = completed.expect("the default profile runs the sink").stdout;
    assert_eq!(all.len(), LINES * (LINE + 1));
    println!("sink.ts under the default profile: peak {control_peak} bytes");
    assert!(
        control_peak > QUOTA + SLACK,
        "the control peaked at {control_peak} bytes, which the bound above admits"
    );
}
