//! The verifier must reject missing guards when the checker accepts the sources.
//! Corpus tests cover execution. This test verifies malformed member-read and field-pattern LIR.
//! Measured debug test-suite cost: 0.01 s.

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::{check_program, lir as l, SourceFile};

#[test]
fn finished_reference_reads_require_guards_in_lir() {
    for (name, text) in [
        (
            "t77.ts",
            include_str!("../../corpus/trap/t77-generator-done-fixed-reference.ts"),
        ),
        (
            "t79.ts",
            include_str!("../../corpus/trap/t79-generator-done-destructuring.ts"),
        ),
    ] {
        let sources = [SourceFile::new(name, text)];
        let hir = check_program(&sources).unwrap();
        let mut invalid = lower_module(&hir).unwrap();
        verify_module(&invalid).unwrap();
        for function in &mut invalid.functions {
            for block in &mut function.blocks {
                for instruction in &mut block.instructions {
                    instruction
                        .traps
                        .retain(|trap| trap.kind != l::TrapKind::GeneratorDoneValue);
                }
            }
        }
        assert!(verify_module(&invalid).is_err(), "{name}");
    }
}
