//! Immutable module globals need no shared-read trap (compiler.md §304 rule 3).

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::{check_program, lir, SourceFile};

/// Cost: 0.06 seconds for two checks and LIR lowerings; no engine execution.
#[test]
fn only_const_global_narrowing_uses_local_origin_without_a_runtime_check() {
    for (binding, shared) in [("const", false), ("let", true)] {
        let source = format!("class A{{x:i32=1;}} function mk():A|null{{return new A();}} {binding} G:A|null=mk(); export function main():void{{if(G===null){{return;}} print(`${{G.x}}`);}}");
        let hir =
            check_program(&[SourceFile::entry("global.ts", source)]).expect("narrowed global");
        let module = lower_module(&hir).expect("lower global read");
        verify_module(&module).expect("verify global read");
        let conversions: Vec<_> = module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction.kind, lir::InstructionKind::NarrowNonNull(_))
            })
            .collect();
        assert_eq!(conversions.len(), 1);
        assert_eq!(
            conversions[0].kind,
            lir::InstructionKind::NarrowNonNull(if shared {
                lir::NarrowOrigin::SharedRead
            } else {
                lir::NarrowOrigin::Local
            })
        );
        assert_eq!(
            conversions[0]
                .traps
                .iter()
                .any(|trap| trap.kind == lir::TrapKind::SharedNullNarrowing),
            shared
        );
    }
}
