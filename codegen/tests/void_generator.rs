//! Checks the value-less suspension form and the ship result storage.
//! Cost: both tests take 0.02 seconds in a debug run; no execution-tier run repeats the corpus.

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::{check_program, lir as l, SourceFile};

#[test]
fn void_suspensions_use_one_form_and_the_c_result_has_only_done() {
    let source = SourceFile::new(
        "a327-void-generator.ts",
        include_str!("../../corpus/accept/a327-void-generator.ts"),
    );
    let hir = check_program(&[source]).expect("void generators check");
    let lir = lower_module(&hir).expect("void generators lower");
    let suspensions = lir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .filter_map(|block| match &block.terminator {
            l::Terminator::Suspend { kind, .. } => Some(kind),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(suspensions.len(), 3);
    assert!(suspensions
        .iter()
        .all(|kind| matches!(kind, l::SuspendKind::Yield(None))));
    let c = subscript_codegen::emit_c(&hir).expect("void generators emit C");
    assert!(c
        .source
        .contains("typedef struct { int32_t done; } SubIR_void;"));
    assert!(!c.source.contains(".value"));
    assert!(c
        .source
        .lines()
        .any(|line| line.contains("->resume(ctx,") && line.contains(", NULL)")));
}

#[test]
fn a_value_less_suspension_in_a_nonvoid_generator_is_invalid_lir() {
    let hir = check_program(&[SourceFile::new(
        "yield.ts",
        "function* numbers(): Generator<i32> { yield 1; } export function main(): void {}",
    )])
    .expect("valued generator checks");
    let mut lir = lower_module(&hir).expect("valued generator lowers");
    let kind = lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .find_map(|block| match &mut block.terminator {
            l::Terminator::Suspend { kind, .. } => Some(kind),
            _ => None,
        })
        .expect("generator suspension");
    *kind = l::SuspendKind::Yield(None);
    let findings = verify_module(&lir).expect_err("the value-less suspension is invalid");
    assert!(findings.iter().any(|finding| finding
        .message
        .contains("value-less yield requires a generator of void")));
}
