//! The adversarial list of the sandbox profile
//! (`specs/blocks/compiler.md` §109.8 criterion 4).
//!
//! Five shapes. Each one is built here, not read from the corpus, and each
//! one goes through the checker under the profile. A shape that checks
//! clean then runs on both tiers under the profile.
//!
//! | Shape | Must |
//! |---|---|
//! | a 1,048,577-byte source | reject S026 |
//! | a 257-deep bracket source | reject S026 |
//! | recursion with no base case | trap `stack-budget` on both tiers |
//! | an allocation loop that keeps every block live | trap `allocation-quota` on both tiers |
//! | `Context.fromBytes` of forged bytes | reject S024 |
//!
//! Every shape carries a control. A reject shape's control is the same
//! source under the default profile, which must check clean. A trap
//! shape's control is the same program with a base case or a bounded
//! loop, which must run clean under the profile.

use subscript_codegen::{run_c_aot_configured, run_jit_configured, RunConfig, RunError};
use subscript_compiler::{
    check_program_with, on_the_compile_thread, CheckOptions, Profile, RuleCode, SourceFile,
};
use subscript_runtime::TrapKind;

/// §109.2 S026: the byte limit of one source file.
const SOURCE_BYTE_LIMIT: usize = 1_048_576;

/// §109.2 S026: the bracket-depth limit.
const BRACKET_DEPTH_LIMIT: usize = 256;

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

/// A program whose bracket depth reaches `depth`: one parenthesized
/// integer inside the entry's braces, which are the first level.
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
    assert_profile_rejects("1,048,577-byte source", &source, RuleCode::S026);
    // The limit itself is accepted, so the rejection is the extra byte.
    let at_the_limit = source_of_length(SOURCE_BYTE_LIMIT);
    assert_eq!(
        check_under(Profile::Sandbox, &at_the_limit),
        Ok(()),
        "the profile rejected a source of exactly the limit"
    );
    println!("1,048,576-byte source: the profile accepts it");
}

#[test]
fn a_source_one_bracket_over_the_limit_rejects_s026() {
    let source = source_of_depth(BRACKET_DEPTH_LIMIT + 1);
    assert_profile_rejects("257-deep bracket source", &source, RuleCode::S026);
    // §109.2 rule 2: a parenthesis is a bracket token and an expression
    // node, and the declaration statement and the literal are nodes of
    // their own, so the nesting guard binds one level before the bracket
    // count does. 255 is the deepest parenthesis source the profile
    // accepts.
    let at_the_limit = source_of_depth(BRACKET_DEPTH_LIMIT - 1);
    assert_eq!(
        check_under(Profile::Sandbox, &at_the_limit),
        Ok(()),
        "the profile rejected the deepest accepted parenthesis source"
    );
    assert_eq!(
        check_under(Profile::Sandbox, &source_of_depth(BRACKET_DEPTH_LIMIT)),
        Err(RuleCode::S026),
        "one level more must report"
    );
    println!("255-deep bracket source: the profile accepts it; 256 reports");
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

/// §109.2 rule 2: the four shapes the amendment names. None of the first
/// three opens a bracket token, so S026's bracket count sees nothing.
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

/// §109.2 S026: the token limit is the proxy that bounds the parser.
///
/// Type arguments and assignment chains open no bracket, so the bracket
/// count sees nothing and the parser recurses once for each level before
/// any checker rule can run. Each source here is far over the limit, so
/// S026 reports before the parser; each control is under it, so the
/// parser runs the whole nest and the nesting guard reports instead.
///
/// The control depth is a sixteenth of the limit. The deepest nest the
/// limit admits is 56,160 type-argument levels, which this compiler
/// parses in 899 MB of stack unoptimized and 298 MB optimized. The
/// compile thread holds both (§109.2a), and the control holds them with
/// a margin over 8x.
#[test]
fn a_source_at_the_token_limit_parses_inside_the_compile_thread() {
    /// §109.2 S026: the token limit of one file.
    const TOKEN_COUNT_LIMIT: usize = 131_072;
    /// The control depth: the parser runs the whole nest below it.
    const CONTROL_LEVELS: usize = TOKEN_COUNT_LIMIT / 16;
    // Two tokens for each level, and five tokens for `const deep: … = [];`
    // outside the nest.
    let type_arguments = |levels: usize| {
        format!(
            "export function main(): void {{\n  const deep: {}i32{} = [];\n  print(`${{deep.length}}`);\n}}\n",
            "Array<".repeat(levels),
            ">".repeat(levels)
        )
    };
    let assignments = |levels: usize| {
        format!(
            "export function main(): void {{\n  let a: i32 = 0;\n  {}1;\n  print(`${{a}}`);\n}}\n",
            "a = ".repeat(levels)
        )
    };
    for (shape, source) in [
        ("type arguments", type_arguments(TOKEN_COUNT_LIMIT)),
        ("assignment chain", assignments(TOKEN_COUNT_LIMIT)),
    ] {
        assert_eq!(
            check_under(Profile::Sandbox, &source),
            Err(RuleCode::S026),
            "{shape}: the token limit must report"
        );
        println!("{shape} at {TOKEN_COUNT_LIMIT} levels: S026 before the parser");
    }
    // The control: the control depth is under the token limit, so the
    // parser runs the whole nest and the nesting guard reports.
    for (shape, source) in [
        ("type arguments", type_arguments(CONTROL_LEVELS)),
        ("assignment chain", assignments(CONTROL_LEVELS)),
    ] {
        assert_eq!(
            check_under(Profile::Sandbox, &source),
            Err(RuleCode::S026),
            "{shape}: the nesting guard must report"
        );
        println!(
            "{shape} at {CONTROL_LEVELS} levels: the parser returns and the nesting guard reports"
        );
    }
}
