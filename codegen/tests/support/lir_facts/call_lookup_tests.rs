//! Violating forms for the facts that resolve a callee symbol.
//!
//! Each clean witness carries a fact only when the callee lookup resolves.
//! If a lookup miss becomes a skip again, each test fails.

use super::*;
use subscript_compiler::{check_program, SourceFile};

const FILE: &str = "call_lookup.ts";

fn checked(source: &str) -> (hir::Module, l::Module) {
    let hir = check_program(&[SourceFile::new(FILE, source)]).expect("lookup witness checks");
    let lir = subscript_codegen::lir::lower_module(&hir)
        .unwrap_or_else(|error| panic!("lookup witness lowers: {error:?}\n{source}"));
    (hir, lir)
}

/// The direct call instruction at `line` in the witness file.
fn direct_call(lir: &mut l::Module, line: u32) -> &mut l::Instruction {
    lir.functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| {
            instruction.pos.file == FILE
                && instruction.pos.line == line
                && matches!(&instruction.kind, l::InstructionKind::Call(target)
                    if matches!(target.kind, l::CallTargetKind::Function(_)))
        })
        .expect("direct module-function call")
}

#[test]
fn a_module_function_call_with_a_dropped_operand_is_a_finding() {
    let source = "function add(a: i32, b: i32): i32 { return a + b; }
export function main(): void {
  add(1, 2);
}";
    let (hir, mut lir) = checked(source);
    assert_eq!(dropped_facts(&hir, &lir), Vec::<String>::new());

    let call = direct_call(&mut lir, 3);
    assert_eq!(call.operands.len(), 2, "the call carries both arguments");
    call.operands.pop();
    let findings = dropped_facts(&hir, &lir);
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("call operand count 2 is absent from LIR")),
        "{findings:?}"
    );
}

#[test]
fn a_trapping_default_at_a_module_function_call_is_a_finding() {
    let source = "const table: i32[] = [1];
function pick(i: i32 = table[3]): i32 { return i; }
export function main(): void {
  pick();
}";
    let (hir, mut lir) = checked(source);
    assert_eq!(dropped_facts(&hir, &lir), Vec::<String>::new());

    // The default argument evaluates at the call site. Drop its read check.
    let mut removed = 0;
    for instruction in lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
    {
        let before = instruction.traps.len();
        instruction.traps.retain(|trap| {
            !(matches!(trap.kind, l::TrapKind::IndexRead)
                && trap.pos.file == FILE
                && trap.pos.line == 2)
        });
        removed += before - instruction.traps.len();
    }
    assert_eq!(removed, 1, "the default carries one read check");
    let findings = dropped_facts(&hir, &lir);
    assert!(
        findings
            .iter()
            .any(|finding| finding.starts_with(&format!("{FILE}:2:"))
                && finding.contains("trap \"IndexRead\" carries 0 site(s); HIR requires 1")),
        "{findings:?}"
    );
}

#[test]
fn a_callee_symbol_that_names_no_function_is_malformed_hir() {
    let source = "function add(a: i32, b: i32): i32 { return a + b; }
export function main(): void {
  add(1, 2);
}";
    let (mut hir, lir) = checked(source);
    let main = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main")
        .symbol
        .clone();
    let add = hir
        .functions
        .iter_mut()
        .find(|function| function.name == "add")
        .expect("add");
    // The call still names the original symbol; no declaration carries it now.
    add.symbol = main;
    let findings = dropped_facts(&hir, &lir);
    let malformed = findings
        .iter()
        .filter(|finding| {
            finding.starts_with(&format!("{FILE}:3:"))
                && finding.contains("malformed HIR: callee symbol")
        })
        .count();
    assert_eq!(
        malformed, 2,
        "operand count and parameter defaults: {findings:?}"
    );
}
