//! The run-time check of the allocation fact (`compiler.md` §342 rule 10a).
//!
//! The fact of an instruction comes from the operation table and the LIR.
//! The Context counts its allocations. The two are separate derivations,
//! so a wrong table value is an `Internal` runtime error here.

use super::*;

impl Interpreter<'_> {
    /// Executes one instruction. If the instruction states "cannot
    /// allocate" and the Context allocation count changes while it
    /// executes, the result is an `Internal` runtime error that names the
    /// instruction kind or the operation.
    pub(super) fn execute_checked(
        &mut self,
        frame: *mut Frame,
        function: &l::Function,
        block: l::BlockId,
        index: usize,
        instruction: &l::Instruction,
    ) -> Result<(), InterpretError> {
        if self
            .allocation
            .instruction_can_allocate(function.id, block, index)
        {
            return self.execute_instruction(frame, function, instruction);
        }
        let before = self.context.successful_allocations();
        let outcome = self.execute_instruction(frame, function, instruction);
        let allocations = self.context.successful_allocations().wrapping_sub(before);
        if allocations == 0 || is_internal(&outcome) {
            return outcome;
        }
        Err(InterpretError::Trap {
            kind: RuntimeTrapKind::Internal.rule().to_string(),
            runtime_kind: Some(RuntimeTrapKind::Internal),
            pos: instruction.pos.clone(),
            message: format!(
                "{} states \"cannot allocate\" and made {allocations} Context allocations \
                 (compiler.md §342 rule 10a)",
                self.describe_allocation_site(&instruction.kind)
            ),
        })
    }

    /// Names the instruction kind, or the operation of a call.
    fn describe_allocation_site(&self, kind: &l::InstructionKind) -> String {
        let l::InstructionKind::Call(target) = kind else {
            let text = format!("{kind:?}");
            let name = text.split(['(', ' ', '{']).next().unwrap_or("");
            return format!("instruction {name}");
        };
        match &target.kind {
            l::CallTargetKind::Intrinsic(intrinsic) => {
                let name = self
                    .module
                    .intrinsic_operations
                    .iter()
                    .find(|row| {
                        row.family == intrinsic.family && row.operation == intrinsic.operation
                    })
                    .map_or("<unknown>", |row| row.semantic_name.as_str());
                format!("operation {:?}.{name}", intrinsic.family)
            }
            l::CallTargetKind::BuiltinMethod(method) => format!("built-in method {method:?}"),
            other => format!("call {other:?}"),
        }
    }
}

/// True for an `Internal` error, which an inner check already reported.
fn is_internal(outcome: &Result<(), InterpretError>) -> bool {
    matches!(
        outcome,
        Err(InterpretError::Trap {
            runtime_kind: Some(RuntimeTrapKind::Internal),
            ..
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
export function main(): void {
  const s: string = "ab";
  print(s.toUpperCase());
}
"#;

    fn lower() -> l::Module {
        let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "allocation.ts",
            SOURCE,
        )])
        .expect("the program checks");
        crate::lir::lower_module(&hir).expect("the program lowers")
    }

    /// The control: the table states the fact that the runtime has.
    #[test]
    fn a_true_table_runs_without_an_internal_error() {
        assert_eq!(interpret(&lower()).expect("the program runs"), b"AB\n");
    }

    /// The firing control: the module states "cannot allocate" for an
    /// operation that allocates a string.
    #[test]
    fn an_allocating_operation_that_states_cannot_allocate_is_internal() {
        let mut module = lower();
        let row = module
            .intrinsic_operations
            .iter_mut()
            .find(|row| {
                row.family == l::IntrinsicFamily::String && row.semantic_name == "ToUpperCase"
            })
            .expect("the table lists toUpperCase");
        assert!(row.can_allocate);
        row.can_allocate = false;
        let error = interpret(&module).expect_err("the check fires");
        let InterpretError::Execution { source, .. } = error else {
            panic!("expected an execution error, got {error:?}");
        };
        match *source {
            InterpretError::Trap {
                runtime_kind: Some(RuntimeTrapKind::Internal),
                message,
                ..
            } => assert!(
                message.contains("operation String.ToUpperCase states \"cannot allocate\""),
                "{message}"
            ),
            other => panic!("expected an Internal trap, got {other:?}"),
        }
    }
}
