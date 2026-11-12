//! A default call takes its own count of each argument (compiler.md
//! §256).
//!
//! Cost: six checker calls and six lowerings with the LIR verifier, no
//! native build; 0.04 s on an arm64 host.

use subscript_codegen::lir::lower_module;
use subscript_compiler::lir as l;
use subscript_compiler::{check_program, SourceFile};

const DECLARATIONS: &str = "async function v(): Promise<i32> { return 1; }\n\
function* gen(): Generator<i32> { yield 10; }\n\
async function w(p: Promise<i32>, k: i32 = 2): Promise<i32> { return (await p) + k; }\n\
function take(g: Generator<i32>, k: i32 = 1): i32 { let t = k; for (const n of g) { t += n; } return t; }\n\
async function cnt(xs: Promise<i32>[], n: i32 = xs.length): Promise<i32> { let t = n; for (const h of xs) { t += await h; } return t; }\n\
function take3(g: Generator<i32>, k: i32 = 1, m: i32 = 2): i32 { let t = k + m; for (const n of g) { t += n; } return t; }\n";

/// The LIR of `main`, after the LIR verifier accepts the module.
fn main_of(body: &str) -> l::Function {
    let source =
        format!("{DECLARATIONS}export async function main(): Promise<void> {{ {body} }}\n");
    let hir = check_program(&[SourceFile::entry("count.ts", source.as_str())]).expect("checks");
    let lir = lower_module(&hir).unwrap_or_else(|error| panic!("{body}: {error}"));
    lir.functions
        .into_iter()
        .find(|function| function.source_name == "main")
        .expect("main")
}

/// The count of each kind of count instruction in `function`.
fn counts(function: &l::Function) -> (usize, usize) {
    let instructions = function.blocks.iter().flat_map(|block| &block.instructions);
    let retains = instructions
        .clone()
        .filter(|i| {
            matches!(
                i.kind,
                l::InstructionKind::AsyncHandleRetain | l::InstructionKind::AsyncHandleArrayRetain
            )
        })
        .count();
    let releases = instructions
        .filter(|i| {
            matches!(
                i.kind,
                l::InstructionKind::AsyncHandleRelease
                    | l::InstructionKind::AsyncHandleArrayRelease
            )
        })
        .count();
    (retains, releases)
}

/// Rule 1: a fresh counted argument that a default call and the callee
/// both take lowers to LIR that the verifier accepts.
#[test]
fn a_default_call_takes_its_own_count() {
    for body in [
        "print(`${await w(v())}`);",
        "print(`${take(gen())}`);",
        "print(`${await cnt([v(), v()])}`);",
    ] {
        main_of(body);
    }
}

/// The control: a read of a local takes a count of its own already, so
/// its LIR has the count instructions it had before rule 1.
#[test]
fn a_local_argument_keeps_its_lir() {
    let main = main_of("const h = v(); print(`${await w(h)}`);");
    assert_eq!(counts(&main), CONTROL_COUNTS);
}

/// The retains and the releases of `main` in the control, measured with
/// the lowering before rule 1.
const CONTROL_COUNTS: (usize, usize) = (2, 2);

/// Rule 2: the count instructions of `main` balance the passes. `w(v())`
/// passes the handle to one default call and to the callee, and
/// `take3(gen())` passes the generator to two default calls and to the
/// callee. Removing the release or one retain changes these counts.
#[test]
fn each_default_call_takes_two_retains_and_one_release() {
    assert_eq!(counts(&main_of("print(`${await w(v())}`);")), W_COUNTS);
    assert_eq!(counts(&main_of("print(`${take3(gen())}`);")), TAKE3_COUNTS);
}

/// The retains and the releases of `main` with `w(v())`, measured.
const W_COUNTS: (usize, usize) = (2, 1);

/// The retains and the releases of `main` with `take3(gen())`, measured.
const TAKE3_COUNTS: (usize, usize) = (4, 2);
