//! Verifier checks for calls of the running lambda (compiler.md §284).
//! Cost: one corpus lowering and four verifier calls.

use super::*;

#[test]
fn self_calls_require_the_current_function_environment_and_argument_types() {
    let module = lower_entry(&corpus::corpus_accept(), "a449-a-local-lambda-calls-itself");
    let (function_index, block_index, instruction_index) = module
        .functions
        .iter()
        .enumerate()
        .find_map(|(f, function)| {
            function.blocks.iter().enumerate().find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .enumerate()
                    .find_map(|(i, instruction)| {
                        matches!(&instruction.kind, InstructionKind::Call(target)
                    if matches!(target.kind, lir::CallTargetKind::LambdaSelf(_))
                    && instruction.operands.len() == 2
                    && target.parameter_types == vec![ValueType::Data(Type::I32); 2])
                        .then_some((f, b, i))
                    })
            })
        })
        .expect("borrowed capture self-call");
    for violation in [0, 1, 2, 3] {
        let mut invalid = module.clone();
        let instruction = &mut invalid.functions[function_index].blocks[block_index].instructions
            [instruction_index];
        match violation {
            0 => {
                let InstructionKind::Call(target) = &mut instruction.kind else {
                    unreachable!()
                };
                target.kind =
                    lir::CallTargetKind::LambdaSelf(module.functions[function_index - 1].id);
            }
            1 => instruction.operands[0] = instruction.operands[1].clone(),
            3 => {
                let InstructionKind::Call(target) = &mut instruction.kind else {
                    unreachable!()
                };
                target.kind = lir::CallTargetKind::Function(module.functions[function_index].id);
            }
            _ => {
                instruction.operands[1] = Operand::Constant(lir::Constant {
                    ty: Type::Bool,
                    kind: lir::ConstantKind::Boolean(false),
                })
            }
        }
        let findings = verify_module(&invalid).expect_err("self-call violation");
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains(if violation == 2 {
                    "call signature disagrees with its target"
                } else {
                    "call target identity/signature is invalid"
                })),
            "{findings:?}"
        );
    }
}
