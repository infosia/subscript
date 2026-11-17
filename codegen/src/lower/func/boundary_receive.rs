//! Read-back helpers for compiler.md §325.
use super::*;
use subscript_compiler::crossing::{PointerPlan, WriteBack};

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn boundary_data(&mut self, bytes: &[u8]) -> Result<Value, String> {
        let data = self.ml.literal_data(bytes)?;
        let global = self.ml.module.declare_data_in_func(data, self.builder.func);
        Ok(self.builder.ins().global_value(types::I64, global))
    }

    pub(super) fn capture_input_arrays(
        &mut self,
        plan: &subscript_compiler::crossing::WriteBackPlan,
        source: Value,
    ) -> Result<Value, String> {
        let offsets = crate::lower::receive::input_arrays(&self.ml.layouts, plan)?;
        let Some(last) = offsets.iter().max() else {
            return Ok(self.iconst(types::I64, 0));
        };
        let original = self.stack_slot(
            last.checked_add(8)
                .ok_or_else(|| internal("input array size overflow"))?,
            8,
        );
        for offset in offsets {
            let value = self
                .builder
                .ins()
                .load(types::I64, flags(), source, offset as i32);
            self.builder
                .ins()
                .store(flags(), value, original, offset as i32);
        }
        Ok(original)
    }

    pub(super) fn receive_member(
        &mut self,
        write: &WriteBack,
        images: (Value, Value, Value),
        destination: Value,
        label: &str,
        pos: &Pos,
    ) -> Result<(), String> {
        let (scratch, snapshot, original) = images;
        let path = self.boundary_data(label.as_bytes())?;
        let length = self.iconst(types::I32, label.len() as i64);
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        match write {
            WriteBack::Pointer { nullable } => {
                let value = self.builder.ins().load(types::I64, flags(), scratch, 0);
                let nullable = self.iconst(types::I32, i64::from(*nullable));
                self.call_runtime(
                    self.ml.rt.boundary_pointer,
                    &[self.ctx, value, nullable, path, length, position],
                    false,
                )?;
                let zero = self.iconst(types::I64, 0);
                self.builder.ins().store(flags(), zero, destination, 0);
            }
            WriteBack::Pair {
                pointer_first,
                element,
            } => {
                let schema = crate::lower::receive::schema(self.ml.lir, &self.ml.layouts, element)?;
                let schema = self.boundary_data(&schema)?;
                let pointer_offset = if *pointer_first { 0 } else { 8 };
                let count_offset = if *pointer_first { 8 } else { 0 };
                let data = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), scratch, pointer_offset);
                let count = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), scratch, count_offset);
                let earlier = self.stack_slot(24, 8);
                let old_data =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), snapshot, pointer_offset);
                let old_count =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), snapshot, count_offset);
                let old_array = self.builder.ins().load(types::I64, flags(), original, 0);
                self.builder.ins().store(flags(), old_data, earlier, 0);
                self.builder.ins().store(flags(), old_count, earlier, 8);
                self.builder.ins().store(flags(), old_array, earlier, 16);
                let array = self
                    .call_runtime(
                        self.ml.rt.boundary_receive,
                        &[
                            self.ctx, data, count, schema, path, length, position, earlier,
                        ],
                        false,
                    )?
                    .ok_or_else(|| internal("received pair has no array"))?;
                self.builder.ins().store(flags(), array, destination, 0);
            }
            _ => return Err(internal("invalid received member")),
        }
        Ok(())
    }

    pub(super) fn snapshot_script_pointer(
        &mut self,
        plan: &PointerPlan,
        source: Value,
        pos: &Pos,
    ) -> Result<(), String> {
        let Some(watch) = &plan.watch else {
            return Ok(());
        };
        let class_name = plan.path.clone();
        let count = self.iconst(types::I64, 1);
        self.watch_memory(watch, &class_name, source, count, false, pos)
    }

    pub(super) fn watch_recursive_memory(
        &mut self,
        watch: &subscript_compiler::crossing::WatchPlan,
        name: &str,
        source: Value,
        count: Value,
        indexed: bool,
        pos: &Pos,
    ) -> Result<(), String> {
        if self
            .boundary_targets
            .as_ref()
            .is_some_and(|targets| targets.elements != 0)
        {
            return Ok(());
        }
        let schema = crate::lower::receive::watch_schema(self.ml.lir, &self.ml.layouts, watch)?;
        let schema = self.boundary_data(&schema)?;
        let targets = self
            .boundary_targets
            .as_ref()
            .ok_or_else(|| internal("pointer snapshot has no call"))?;
        let label = format!("{}.{name}", targets.callee);
        let path = self.boundary_data(label.as_bytes())?;
        let length = self.iconst(types::I32, label.len() as i64);
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        let indexed = self.iconst(types::I32, i64::from(indexed));
        let token = self
            .call_runtime(
                self.ml.rt.boundary_watch,
                &[
                    self.ctx, source, count, indexed, schema, path, length, position,
                ],
                false,
            )?
            .ok_or_else(|| internal("pointer snapshot has no token"))?;
        self.scratch_allocation_check()?;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        self.builder.ins().stack_store(token, slot, 0);
        self.boundary_targets
            .as_mut()
            .ok_or_else(|| internal("pointer snapshot has no call"))?
            .watches
            .push(slot);
        Ok(())
    }

    pub(super) fn finish_boundary_watches(
        &mut self,
        targets: &BoundaryTargets,
    ) -> Result<(), String> {
        if targets.watches.is_empty() {
            return self.finish_inline_watches(targets);
        }
        {
            use cranelift_codegen::cursor::{Cursor, FuncCursor};
            let mut cursor =
                FuncCursor::new(self.builder.func).at_first_insertion_point(targets.start);
            let null = cursor.ins().iconst(types::I64, 0);
            for slot in &targets.watches {
                cursor.ins().stack_store(null, *slot, 0);
            }
        }
        for slot in &targets.watches {
            let snapshot = self.builder.ins().stack_load(types::I64, *slot, 0);
            self.call_runtime(
                self.ml.rt.boundary_watch_finish,
                &[self.ctx, snapshot],
                false,
            )?;
        }
        self.finish_inline_watches(targets)
    }
}
