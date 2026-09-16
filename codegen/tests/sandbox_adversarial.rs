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
use subscript_compiler::{check_program_with, CheckOptions, Profile, RuleCode, SourceFile};
use subscript_runtime::TrapKind;

/// §109.2 S026: the byte limit of one source file.
const SOURCE_BYTE_LIMIT: usize = 1_048_576;

/// §109.2 S026: the bracket-depth limit.
const BRACKET_DEPTH_LIMIT: usize = 256;

fn files(source: String) -> Vec<SourceFile> {
    vec![SourceFile::new("adversarial.ts", source)]
}

/// Checks `source` under `profile` and returns the first diagnostic code.
fn check_under(profile: Profile, source: &str) -> Result<(), RuleCode> {
    let options = CheckOptions::with_profile(profile);
    match check_program_with(&files(source.to_string()), &options) {
        Ok(_) => Ok(()),
        Err(diagnostics) => Err(diagnostics
            .first()
            .expect("a rejection carries a diagnostic")
            .code),
    }
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
    let at_the_limit = source_of_depth(BRACKET_DEPTH_LIMIT);
    assert_eq!(
        check_under(Profile::Sandbox, &at_the_limit),
        Ok(()),
        "the profile rejected a source of exactly the limit"
    );
    println!("256-deep bracket source: the profile accepts it");
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
