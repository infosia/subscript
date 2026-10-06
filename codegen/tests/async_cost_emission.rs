//! Async cost regressions that whole-program output checks cannot detect.

use subscript_codegen::{emit_c, lir::lower_module};
use subscript_compiler::{
    check_program,
    lir::{CountAction, InstructionKind, TrapKind},
    SourceFile,
};

#[test]
fn quiet_completion_blocks_emit_no_pending_word_check() {
    for (name, source) in [
        (
            "settled-awaits",
            include_str!("../../benchmarks/workloads/subscript/async/settled-awaits.ts"),
        ),
        (
            "deep-chains",
            include_str!("../../benchmarks/workloads/subscript/async/deep-chains.ts"),
        ),
    ] {
        let hir = check_program(&[SourceFile::new(name, source)]).expect("checked workload");
        let lir = lower_module(&hir).expect("verified LIR");
        let c = emit_c(&hir).expect("emitted workload").source;
        let mut quiet_blocks = 0;
        for function in &lir.functions {
            let signature = format!(
                "static uint8_t sub_f{}_resume(void* ctx, void* raw_frame, void* coroutine_out) {{",
                function.id.0
            );
            let Some((_, body)) = c.split_once(&signature) else {
                continue;
            };
            let function_body = body.split("\n}\n").next().expect("function end");
            for block in &function.blocks {
                let Some(instruction) = block.instructions.iter().find(|instruction| {
                    matches!(instruction.kind, InstructionKind::AwaitRaise)
                        && instruction.raise_edge().is_none()
                }) else {
                    continue;
                };
                assert_eq!(
                    instruction.count_action,
                    Some(CountAction::Uncounted),
                    "{name}: quiet scalar completion has a counted action"
                );
                assert!(
                    instruction
                        .traps
                        .iter()
                        .all(|trap| trap.kind != TrapKind::Call),
                    "{name}: uncounted completion has a count trap"
                );
                quiet_blocks += 1;
                let label = format!("\nb{}:\n", block.id.0);
                let (_, body) = function_body.split_once(&label).expect("completion block");
                let body = body.split("\n    }").next().expect("block end");
                assert!(
                    !body.contains("if (*(const uint32_t*)ctx != 0u)"),
                    "{name}: quiet completion adds a Context read: {body}"
                );
            }
        }
        assert!(
            quiet_blocks > 0,
            "{name}: the control must exercise quiet awaits"
        );
    }
}

#[test]
fn handle_array_release_uses_the_direct_leaf_abi() {
    let source = include_str!("../../benchmarks/workloads/subscript/async/held-handles.ts");
    let hir = check_program(&[SourceFile::new("held-handles", source)]).expect("checked workload");
    let c = emit_c(&hir).expect("emitted workload").source;
    assert!(c.contains("subscript_rt_async_release_array(ctx, roots.v7,"));
    assert!(!c.contains("subscript_rt_counted_value("));
}
