//! Inline frame counts (§116.2 rule 6 and §314 rule 1).

use super::*;

impl<M: Module> Body<'_, '_, '_, '_, M> {
    pub(super) fn frame_count(
        &mut self,
        handle: Value,
        offset: u32,
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
            .load(types::I32, MemFlags::trusted(), handle, offset as i32);
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
                .store(MemFlags::trusted(), count, handle, offset as i32);
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
                .store(MemFlags::trusted(), count, handle, offset as i32);
        }
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::lower::func::take_defined_function_texts;
    use crate::lower::{dev_flags, lower_lir_module_with, LowerOptions};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{default_libcall_names, FuncOrDataId, Module};
    use subscript_compiler::{check_program, SourceFile};

    // Cost: four checker/JIT lowerings; measured debug test: 0.09 s.
    #[test]
    fn generator_counts_lower_inline_without_description_dispatch() {
        for (source, description_call) in [
            (include_str!("../../../tests/fixtures/generator-count.ts"), false),
            ("function hold(xs: Generator<i32>[]): void { const copy = xs; } export function main(): void {}", true),
            ("function hold(xs: FixedArray<Generator<i32>, 1>): void { const copy = xs; } export function main(): void {}", true),
            ("function* leaf(): Generator<i32> { yield 1; } function* outer(g: Generator<i32>): Generator<Generator<i32>> { yield g; } function probe(): void { const g = leaf(); const o = outer(g); const result = o.next(); const copy = result; print(`${copy.value.next().value}`); } export function main(): void {}", true),
        ] {
            let hir = check_program(&[SourceFile::new("generator-count.ts", source)]).expect("checked generator");
            let lir = crate::lir::lower_module(&hir).expect("generator LIR");
            let isa = cranelift_native::builder().expect("host ISA")
                .finish(dev_flags().expect("dev flags")).expect("ISA flags");
            let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
            let _ = take_defined_function_texts();
            lower_lir_module_with(&mut module, &lir, LowerOptions::default()).expect("generator JIT lowering");
            let functions = take_defined_function_texts();
            let Some(FuncOrDataId::Func(id)) = module.declarations().get_name("subscript_rt_counted_value") else {
                panic!("counted runtime declaration");
            };
            let external = format!("u0:{} ", id.as_u32());
            let has_call = functions.iter().any(|(_, text)| text.lines().any(|line| {
                let Some((reference, declaration)) = line.trim().split_once(" = ") else { return false; };
                declaration.starts_with(&external) && text.contains(&format!("call {reference}("))
            }));
            // SAFETY: no finalized function address escapes this test.
            unsafe { module.free_memory() };
            assert_eq!(has_call, description_call, "{functions:#?}");
        }
    }
}
