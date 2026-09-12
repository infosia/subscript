//! Branch arguments, returns, and block terminators.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn branch_arguments(&mut self, arguments: &[l::Operand]) -> Result<Vec<BlockArg>, String> {
        let mut result = Vec::new();
        for argument in arguments {
            result.extend(rv_args(self.operand(argument)?));
        }
        Ok(result)
    }

    fn emit_return(&mut self, value: Option<&l::Operand>, _pos: &Pos) -> Result<(), String> {
        if let Some(kind) = self.coroutine {
            let frame = self
                .frame
                .ok_or_else(|| internal("coroutine return has no frame"))?;
            if kind == CoroutineKind::Async {
                if let Some(value) = value {
                    let value = self.operand(value)?;
                    let output = self
                        .out
                        .ok_or_else(|| internal("async return has no output"))?;
                    let return_type = self.function.return_type.clone();
                    self.store_data(&return_type, output, 0, value)?;
                }
            }
            let done = self.iconst(types::I32, COROUTINE_DONE);
            self.builder.ins().store(flags(), done, frame, 0);
            self.pop_shadow()?;
            let one = self.iconst(types::I8, 1);
            self.builder.ins().return_(&[one]);
            return Ok(());
        }
        let value = value.map(|value| self.operand(value)).transpose()?;
        let returns = match (self.ml.layouts.repr(&self.function.return_type)?, value) {
            (Repr::None, _) => Vec::new(),
            (Repr::Scalar(_), Some(RV::Scalar(value))) => vec![value],
            (Repr::Pair, Some(RV::Pair(code, env))) => vec![code, env],
            (Repr::Agg { size, align }, Some(RV::Aggregate(source))) => {
                let destination = self
                    .sret
                    .ok_or_else(|| internal("aggregate return has no sret"))?;
                self.copy_bytes(destination, source, size, align);
                if let Type::Class(class) = &self.function.return_type {
                    if self.is_value_class(&Type::Class(*class))
                        && boundary_class_contains_pointer(self.ml.lir, *class)?
                    {
                        self.stabilize_boundary_return_value(class.0, destination)?;
                    }
                }
                Vec::new()
            }
            (repr, value) => {
                return Err(internal(format!("return mismatch {repr:?} and {value:?}")))
            }
        };
        self.pop_shadow()?;
        self.builder.ins().return_(&returns);
        Ok(())
    }

    pub(super) fn emit_terminator(
        &mut self,
        block: l::BlockId,
        terminator: &l::Terminator,
    ) -> Result<(), String> {
        match terminator {
            l::Terminator::Branch(target) => {
                let destination = self.blocks[target.block.0 as usize];
                let arguments = self.branch_arguments(&target.arguments)?;
                self.builder.ins().jump(destination, &arguments);
            }
            l::Terminator::ConditionalBranch {
                condition,
                then_target,
                else_target,
            } => {
                let condition_value = self.operand(condition)?;
                let condition = self.expect_scalar(condition_value)?;
                let then_block = self.blocks[then_target.block.0 as usize];
                let else_block = self.blocks[else_target.block.0 as usize];
                let then_arguments = self.branch_arguments(&then_target.arguments)?;
                let else_arguments = self.branch_arguments(&else_target.arguments)?;
                self.builder.ins().brif(
                    condition,
                    then_block,
                    &then_arguments,
                    else_block,
                    &else_arguments,
                );
            }
            l::Terminator::Switch {
                value,
                arms,
                default,
            } => {
                let switch_value = self.operand(value)?;
                let value = self.expect_scalar(switch_value)?;
                let mut next = None;
                for arm in arms {
                    if let Some(block) = next {
                        self.builder.switch_to_block(block);
                    }
                    let case_value = self.constant(&arm.value)?;
                    let case = self.expect_scalar(case_value)?;
                    let matches = self.builder.ins().icmp(IntCC::Equal, value, case);
                    let otherwise = self.builder.create_block();
                    let destination = self.blocks[arm.target.block.0 as usize];
                    let arguments = self.branch_arguments(&arm.target.arguments)?;
                    self.builder
                        .ins()
                        .brif(matches, destination, &arguments, otherwise, &[]);
                    next = Some(otherwise);
                }
                if let Some(block) = next {
                    self.builder.switch_to_block(block);
                }
                let destination = self.blocks[default.block.0 as usize];
                let arguments = self.branch_arguments(&default.arguments)?;
                self.builder.ins().jump(destination, &arguments);
            }
            l::Terminator::Return { value, pos } => self.emit_return(value.as_ref(), pos)?,
            l::Terminator::Unreachable { .. } => {
                let unwind = self.unwind_block();
                self.builder.ins().jump(unwind, &[]);
            }
            l::Terminator::Trap(trap) => {
                self.emit_trap(trap, TrapOperand::Pending)?;
                let unwind = self.unwind_block();
                self.builder.ins().jump(unwind, &[]);
            }
            l::Terminator::Suspend { .. } => self.emit_suspend(block, terminator)?,
        }
        Ok(())
    }
}
