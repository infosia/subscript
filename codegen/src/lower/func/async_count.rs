//! Inline async handle counts (compiler.md §116.2 rule 6).

use super::*;

impl<M: Module> Body<'_, '_, '_, '_, M> {
    pub(super) fn async_count(
        &mut self,
        handle: Value,
        release: Option<(&Pos, &[l::Trap])>,
    ) -> Result<(), String> {
        let count_block = self.builder.create_block();
        let done = self.builder.create_block();
        let slow = release.map(|_| self.builder.create_block());
        let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, handle, 0);
        self.builder
            .ins()
            .brif(nonnull, count_block, &[], slow.unwrap_or(done), &[]);
        self.builder.switch_to_block(count_block);
        let count = self
            .builder
            .ins()
            .load(types::I32, MemFlags::trusted(), handle, 4);
        if let Some((pos, traps)) = release {
            let slow = slow.ok_or_else(|| internal("async release has no runtime block"))?;
            let decrement = self.builder.create_block();
            let shared = self
                .builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThan, count, 1);
            self.builder.ins().brif(shared, decrement, &[], slow, &[]);
            self.builder.switch_to_block(decrement);
            let count = self.builder.ins().iadd_imm(count, -1);
            self.builder
                .ins()
                .store(MemFlags::trusted(), count, handle, 4);
            self.builder.ins().jump(done, &[]);
            self.builder.switch_to_block(slow);
            let pos = self.position_id(pos);
            let pos = self.iconst(types::I32, pos);
            self.call_runtime(self.ml.rt.async_release, &[self.ctx, handle, pos], false)?;
            for trap in traps {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        } else {
            let full = self.builder.ins().icmp_imm(IntCC::Equal, count, -1);
            let incremented = self.builder.ins().iadd_imm(count, 1);
            let count = self.builder.ins().select(full, count, incremented);
            self.builder
                .ins()
                .store(MemFlags::trusted(), count, handle, 4);
        }
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }
}
