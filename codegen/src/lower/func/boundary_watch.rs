//! Inline snapshots of finite watch plans (compiler.md §325 rules 5, 8, and 9).
use super::*;
use crate::lower::watch::{self, Kind, Record};
use subscript_compiler::crossing::WatchPlan;

#[derive(Clone, Debug)]
pub(super) struct Inline {
    slot: StackSlot,
    record: Record,
    indexed: bool,
    path: String,
    pos: Pos,
}
impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn watch_memory(
        &mut self,
        plan: &WatchPlan,
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
        if plan.recursive {
            return self.watch_recursive_memory(plan, name, source, count, indexed, pos);
        }
        let record = watch::record(self.ml.lir, &self.ml.layouts, plan)?;
        let words = if indexed {
            3
        } else {
            record
                .words
                .checked_add(1)
                .ok_or_else(|| internal("watch size overflow"))?
        };
        let size = words
            .checked_mul(8)
            .ok_or_else(|| internal("inline watch size overflow"))?;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            3,
        ));
        let storage = self.builder.ins().stack_addr(types::I64, slot, 0);
        if indexed {
            self.watch_capture_span(source, count, &record, storage, pos)?;
        } else {
            self.builder.ins().store(flags(), source, storage, 0);
            let contents = self.watch_address(storage, 8);
            self.watch_capture(source, &record, contents, pos)?;
        }
        let targets = self
            .boundary_targets
            .as_mut()
            .ok_or_else(|| internal("inline watch has no call"))?;
        let path = format!("{}.{name}", targets.callee);
        targets.inline_watches.push(Inline {
            slot,
            record,
            indexed,
            path,
            pos: pos.clone(),
        });
        Ok(())
    }
    fn watch_address(&mut self, base: Value, offset: u32) -> Value {
        self.builder.ins().iadd_imm(base, i64::from(offset))
    }
    fn watch_capture(
        &mut self,
        source: Value,
        record: &Record,
        storage: Value,
        pos: &Pos,
    ) -> Result<(), String> {
        let populate = self.builder.create_block();
        let done = self.builder.create_block();
        let present = self.builder.ins().icmp_imm(IntCC::NotEqual, source, 0);
        self.builder.ins().brif(present, populate, &[], done, &[]);
        self.builder.switch_to_block(populate);
        self.watch_capture_present(source, record, storage, pos)?;
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }
    fn watch_capture_present(
        &mut self,
        source: Value,
        record: &Record,
        storage: Value,
        pos: &Pos,
    ) -> Result<(), String> {
        let mut values = HashMap::new();
        for entry in &record.entries {
            let address = self.watch_address(source, entry.offset);
            let target = self.watch_address(storage, entry.word * 8);
            match &entry.kind {
                Kind::Slot { .. } => {
                    let before = self.builder.ins().load(types::I64, flags(), address, 0);
                    self.builder.ins().store(flags(), before, target, 0);
                    values.insert(entry.offset, before);
                }
                Kind::Pointer {
                    child,
                    child_word,
                    capture,
                } => {
                    let pointer = if *capture {
                        let value = self.builder.ins().load(types::I64, flags(), address, 0);
                        self.builder.ins().store(flags(), value, target, 0);
                        value
                    } else {
                        *values
                            .get(&entry.offset)
                            .ok_or_else(|| internal("watch pointer lost its saved slot"))?
                    };
                    let child_storage = self.watch_address(storage, child_word * 8);
                    self.watch_capture(pointer, child, child_storage, pos)?;
                }
                Kind::Embedded(child) => self.watch_capture_present(address, child, target, pos)?,
                Kind::Array { element } => {
                    let array = self.builder.ins().load(types::I64, flags(), address, 0);
                    let populate = self.builder.create_block();
                    let empty = self.builder.create_block();
                    let done = self.builder.create_block();
                    let present = self.builder.ins().icmp_imm(IntCC::NotEqual, array, 0);
                    self.builder.ins().brif(present, populate, &[], empty, &[]);
                    self.builder.switch_to_block(empty);
                    let zero = self.iconst(types::I64, 0);
                    self.builder.ins().store(flags(), zero, target, 8);
                    self.builder.ins().jump(done, &[]);
                    self.builder.switch_to_block(populate);
                    let count =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), array, ARRAY_LEN_OFFSET);
                    let data =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), array, ARRAY_DATA_OFFSET);
                    self.watch_capture_span(data, count, element, target, pos)?;
                    self.builder.ins().jump(done, &[]);
                    self.builder.switch_to_block(done);
                }
            }
        }
        Ok(())
    }
    fn watch_capture_span(
        &mut self,
        source: Value,
        count: Value,
        element: &Record,
        storage: Value,
        pos: &Pos,
    ) -> Result<(), String> {
        let populate = self.builder.create_block();
        let empty = self.builder.create_block();
        let done = self.builder.create_block();
        let present = self.builder.ins().icmp_imm(IntCC::NotEqual, source, 0);
        let nonempty = self.builder.ins().icmp_imm(IntCC::NotEqual, count, 0);
        let active = self.builder.ins().band(present, nonempty);
        self.builder.ins().brif(active, populate, &[], empty, &[]);
        self.builder.switch_to_block(empty);
        let zero = self.iconst(types::I64, 0);
        self.builder.ins().store(flags(), zero, storage, 8);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(populate);
        let words = self.builder.ins().imul_imm(count, i64::from(element.words));
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        let buffer = self
            .call_runtime(
                self.ml.rt.boundary_watch_buffer,
                &[self.ctx, words, position],
                false,
            )?
            .ok_or_else(|| internal("watch buffer has no address"))?;
        self.scratch_allocation_check()?;
        self.builder.ins().store(flags(), buffer, storage, 0);
        self.builder.ins().store(flags(), count, storage, 8);
        self.builder.ins().store(flags(), source, storage, 16);
        self.watch_loop(count, |body, index| {
            let offset = body
                .builder
                .ins()
                .imul_imm(index, i64::from(element.words) * 8);
            let target = body.builder.ins().iadd(buffer, offset);
            let offset = body
                .builder
                .ins()
                .imul_imm(index, i64::from(element.stride));
            let source = body.builder.ins().iadd(source, offset);
            body.watch_capture_present(source, element, target, pos)
        })?;
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }
    fn watch_loop(
        &mut self,
        count: Value,
        mut visit: impl FnMut(&mut Self, Value) -> Result<(), String>,
    ) -> Result<(), String> {
        let condition = self.builder.create_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(condition, types::I64);
        let zero = self.iconst(types::I64, 0);
        self.builder.ins().jump(condition, &[BlockArg::Value(zero)]);
        self.builder.switch_to_block(condition);
        let index = self.builder.block_params(condition)[0];
        let more = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, index, count);
        self.builder.ins().brif(more, body, &[], done, &[]);
        self.builder.switch_to_block(body);
        visit(self, index)?;
        let next = self.builder.ins().iadd_imm(index, 1);
        self.builder.ins().jump(condition, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        Ok(())
    }
    pub(super) fn finish_inline_watches(
        &mut self,
        targets: &BoundaryTargets,
    ) -> Result<(), String> {
        if targets.inline_watches.is_empty() {
            return Ok(());
        }
        {
            use cranelift_codegen::cursor::{Cursor, FuncCursor};
            let mut cursor =
                FuncCursor::new(self.builder.func).at_first_insertion_point(targets.start);
            let zero = cursor.ins().iconst(types::I64, 0);
            for watch in &targets.inline_watches {
                let offset = if watch.indexed { 8 } else { 0 };
                cursor.ins().stack_store(zero, watch.slot, offset);
            }
        }
        let populate = self.builder.create_block();
        let done = self.builder.create_block();
        let pending = self.pending_trap_state();
        let clear = self.builder.ins().icmp_imm(IntCC::Equal, pending, 0);
        self.builder.ins().brif(clear, populate, &[], done, &[]);
        self.builder.switch_to_block(populate);
        for watch in &targets.inline_watches {
            let storage = self.builder.ins().stack_addr(types::I64, watch.slot, 0);
            if watch.indexed {
                self.watch_compare_span(
                    &watch.record,
                    storage,
                    &watch.path,
                    &[],
                    &watch.pos,
                    done,
                )?;
            } else {
                let source = self.builder.ins().load(types::I64, flags(), storage, 0);
                let contents = self.watch_address(storage, 8);
                self.watch_compare(
                    &watch.record,
                    (source, contents),
                    &watch.path,
                    &[],
                    &watch.pos,
                    done,
                )?;
            }
        }
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }
    fn watch_compare(
        &mut self,
        record: &Record,
        place: (Value, Value),
        path: &str,
        indices: &[Value],
        pos: &Pos,
        done: Block,
    ) -> Result<(), String> {
        let check = self.builder.create_block();
        let ready = self.builder.create_block();
        let present = self.builder.ins().icmp_imm(IntCC::NotEqual, place.0, 0);
        self.builder.ins().brif(present, check, &[], ready, &[]);
        self.builder.switch_to_block(check);
        self.watch_compare_present(record, place, path, indices, pos, done)?;
        self.builder.ins().jump(ready, &[]);
        self.builder.switch_to_block(ready);
        Ok(())
    }
    fn watch_compare_present(
        &mut self,
        record: &Record,
        place: (Value, Value),
        path: &str,
        indices: &[Value],
        pos: &Pos,
        done: Block,
    ) -> Result<(), String> {
        let (source, storage) = place;
        for entry in &record.entries {
            let address = self.watch_address(source, entry.offset);
            let target = self.watch_address(storage, entry.word * 8);
            let path = format!("{path}.{}", entry.name);
            match &entry.kind {
                Kind::Slot { nullable } => {
                    let before = self.builder.ins().load(types::I64, flags(), target, 0);
                    let after = self.builder.ins().load(types::I64, flags(), address, 0);
                    let changed = self.builder.ins().icmp(IntCC::NotEqual, before, after);
                    let invalid = if *nullable {
                        let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, after, 0);
                        self.builder.ins().band(changed, nonnull)
                    } else {
                        changed
                    };
                    let error = self.builder.create_block();
                    let ready = self.builder.create_block();
                    self.builder.ins().brif(invalid, error, &[], ready, &[]);
                    self.builder.switch_to_block(error);
                    self.watch_error(&path, indices, pos)?;
                    self.builder.ins().jump(done, &[]);
                    self.builder.switch_to_block(ready);
                }
                Kind::Pointer {
                    child, child_word, ..
                } => {
                    let pointer = self.builder.ins().load(types::I64, flags(), target, 0);
                    let child_storage = self.watch_address(storage, child_word * 8);
                    self.watch_compare(child, (pointer, child_storage), &path, indices, pos, done)?;
                }
                Kind::Embedded(child) => {
                    self.watch_compare_present(child, (address, target), &path, indices, pos, done)?
                }
                Kind::Array { element } => {
                    self.watch_compare_span(element, target, &path, indices, pos, done)?
                }
            }
        }
        Ok(())
    }
    fn watch_compare_span(
        &mut self,
        element: &Record,
        storage: Value,
        path: &str,
        indices: &[Value],
        pos: &Pos,
        done: Block,
    ) -> Result<(), String> {
        let count = self.builder.ins().load(types::I64, flags(), storage, 8);
        let populate = self.builder.create_block();
        let ready = self.builder.create_block();
        let nonempty = self.builder.ins().icmp_imm(IntCC::NotEqual, count, 0);
        self.builder.ins().brif(nonempty, populate, &[], ready, &[]);
        self.builder.switch_to_block(populate);
        let buffer = self.builder.ins().load(types::I64, flags(), storage, 0);
        let source = self.builder.ins().load(types::I64, flags(), storage, 16);
        self.watch_loop(count, |body, index| {
            let offset = body
                .builder
                .ins()
                .imul_imm(index, i64::from(element.words) * 8);
            let target = body.builder.ins().iadd(buffer, offset);
            let offset = body
                .builder
                .ins()
                .imul_imm(index, i64::from(element.stride));
            let source = body.builder.ins().iadd(source, offset);
            let mut indices = indices.to_vec();
            indices.push(index);
            body.watch_compare_present(
                element,
                (source, target),
                &format!("{path}[]"),
                &indices,
                pos,
                done,
            )
        })?;
        self.builder.ins().jump(ready, &[]);
        self.builder.switch_to_block(ready);
        Ok(())
    }
    fn watch_error(&mut self, path: &str, indices: &[Value], pos: &Pos) -> Result<(), String> {
        let label = self.boundary_data(path.as_bytes())?;
        let length = self.iconst(types::I32, path.len() as i64);
        let pointer = if indices.is_empty() {
            self.iconst(types::I64, 0)
        } else {
            let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (indices.len() * 8) as u32,
                3,
            ));
            for (index, value) in indices.iter().enumerate() {
                self.builder
                    .ins()
                    .stack_store(*value, slot, (index * 8) as i32);
            }
            self.builder.ins().stack_addr(types::I64, slot, 0)
        };
        let count = self.iconst(types::I32, indices.len() as i64);
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        self.call_runtime(
            self.ml.rt.boundary_watch_error,
            &[self.ctx, label, length, pointer, count, position],
            false,
        )?;
        Ok(())
    }
}
