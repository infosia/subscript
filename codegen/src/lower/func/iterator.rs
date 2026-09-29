//! Iterator creation, bounds, and advance over arrays, maps, and sets.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn iterator_create(
        &mut self,
        kind: l::ForOfKind,
        bound_kind: l::IteratorBoundKind,
        subject: RV,
        subject_ty: &l::ValueType,
        iterator_ty: &l::IteratorType,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.stack_slot(32, 8);
        self.zero_bytes(cursor, 32, 8);
        let subject = match kind {
            l::ForOfKind::FixedArrayValues => self.expect_aggregate(subject)?,
            _ => self.expect_scalar(subject)?,
        };
        self.builder.ins().store(flags(), subject, cursor, 0);
        let fixed = self.iconst(
            types::I64,
            i64::from(bound_kind == l::IteratorBoundKind::Fixed),
        );
        self.builder.ins().store(flags(), fixed, cursor, 24);
        if kind == l::ForOfKind::FixedArrayValues
            && !matches!(subject_ty, l::ValueType::Data(Type::FixedArray(_, _)))
        {
            return Err(internal(
                "fixed-array iterator has no fixed-array subject type",
            ));
        }
        let captured = if let (
            l::ForOfKind::FixedArrayValues,
            l::ValueType::Data(Type::FixedArray(_, count)),
        ) = (kind, subject_ty)
        {
            self.iconst(types::I64, i64::from(*count))
        } else {
            self.iterator_current_bound(cursor, iterator_ty, pos)?
        };
        self.builder.ins().store(flags(), captured, cursor, 16);
        if matches!(
            kind,
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse
        ) {
            let empty = self.builder.ins().icmp_imm(IntCC::Equal, captured, 0);
            let last = self.builder.ins().iadd_imm(captured, -1);
            let exhausted = self.iconst(types::I64, -1);
            let position = self.builder.ins().select(empty, exhausted, last);
            self.builder.ins().store(flags(), position, cursor, 8);
        } else if matches!(
            kind,
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
        ) {
            let start = self.iconst(types::I64, 0);
            let position =
                self.next_live_assoc_position(subject, iterator_ty, start, captured, pos)?;
            self.builder.ins().store(flags(), position, cursor, 8);
        }
        Ok(RV::Aggregate(cursor))
    }

    fn iterator_current_bound(
        &mut self,
        cursor: Value,
        iterator_ty: &l::IteratorType,
        pos: &Pos,
    ) -> Result<Value, String> {
        let subject = self.builder.ins().load(types::I64, flags(), cursor, 0);
        let bound = match iterator_ty.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::ArrayValuesReverse
            | l::ForOfKind::ArrayKeysReverse => {
                self.builder
                    .ins()
                    .load(types::I64, flags(), subject, ARRAY_LEN_OFFSET)
            }
            l::ForOfKind::FixedArrayValues => {
                self.builder.ins().load(types::I64, flags(), cursor, 16)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.assoc_iter_begin,
                    &[self.ctx, subject, position],
                    true,
                )?
                .ok_or_else(|| internal("association iterator bound has no result"))?
            }
            l::ForOfKind::StringCodePoints => self
                .call_runtime(self.ml.rt.str_len, &[self.ctx, subject], false)?
                .ok_or_else(|| internal("string iterator bound has no result"))?,
        };
        Ok(if self.builder.func.dfg.value_type(bound) == types::I32 {
            self.builder.ins().uextend(types::I64, bound)
        } else {
            bound
        })
    }

    pub(super) fn iterator_bound(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let current = self.iterator_current_bound(cursor, iterator_ty, pos)?;
        let captured = self.builder.ins().load(types::I64, flags(), cursor, 16);
        let fixed = self.builder.ins().load(types::I64, flags(), cursor, 24);
        let fixed = self.builder.ins().icmp_imm(IntCC::NotEqual, fixed, 0);
        let bound = self.builder.ins().select(fixed, captured, current);
        Ok(RV::Scalar(self.builder.ins().ireduce(types::I32, bound)))
    }

    fn iterator_effective_bound(
        &mut self,
        cursor: Value,
        iterator_ty: &l::IteratorType,
        captured: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        let captured = if self.builder.func.dfg.value_type(captured) == types::I64 {
            captured
        } else {
            self.builder.ins().uextend(types::I64, captured)
        };
        let current = self.iterator_current_bound(cursor, iterator_ty, pos)?;
        let fixed_limit = self.builder.ins().umin(captured, current);
        let fixed = self.builder.ins().load(types::I64, flags(), cursor, 24);
        let fixed = self.builder.ins().icmp_imm(IntCC::NotEqual, fixed, 0);
        Ok(self.builder.ins().select(fixed, fixed_limit, current))
    }

    fn next_live_assoc_position(
        &mut self,
        subject: Value,
        iterator_ty: &l::IteratorType,
        start: Value,
        bound: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        let (size, align) = self.ml.layouts.size_align(&iterator_ty.element)?;
        let output = self.stack_slot(size.max(1), align.max(1));
        let select = self.iconst(
            types::I32,
            i64::from(iterator_ty.kind == l::ForOfKind::MapValues),
        );
        let diagnostic_position = self.position_id(pos);
        let diagnostic_position = self.iconst(types::I32, diagnostic_position);
        let loop_block = self.builder.create_block();
        let found = self.builder.create_block();
        self.builder.append_block_param(loop_block, types::I64);
        self.builder.append_block_param(found, types::I64);
        self.builder
            .ins()
            .jump(loop_block, &[BlockArg::Value(start)]);
        self.builder.switch_to_block(loop_block);
        let candidate = self.builder.block_params(loop_block)[0];
        let below = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, candidate, bound);
        let inspect = self.builder.create_block();
        self.builder
            .ins()
            .brif(below, inspect, &[], found, &[BlockArg::Value(candidate)]);
        self.builder.switch_to_block(inspect);
        let active = self
            .call_runtime(
                self.ml.rt.assoc_iter_copy,
                &[
                    self.ctx,
                    subject,
                    candidate,
                    select,
                    output,
                    diagnostic_position,
                ],
                true,
            )?
            .ok_or_else(|| internal("association iterator active flag is missing"))?;
        let active = self.builder.ins().icmp_imm(IntCC::NotEqual, active, 0);
        let next = self.builder.create_block();
        self.builder
            .ins()
            .brif(active, found, &[BlockArg::Value(candidate)], next, &[]);
        self.builder.switch_to_block(next);
        let candidate = self.builder.ins().iadd_imm(candidate, 1);
        self.builder
            .ins()
            .jump(loop_block, &[BlockArg::Value(candidate)]);
        self.builder.switch_to_block(found);
        Ok(self.builder.block_params(found)[0])
    }

    pub(super) fn iterator_has_next(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        index: Value,
        bound: Value,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let cursor_position = if matches!(
            iterator_ty.kind,
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
        ) {
            self.builder.ins().uextend(types::I64, index)
        } else {
            self.builder.ins().load(types::I64, flags(), cursor, 8)
        };
        let effective = self.iterator_effective_bound(cursor, iterator_ty, bound, pos)?;
        let condition =
            self.builder
                .ins()
                .icmp(IntCC::UnsignedLessThan, cursor_position, effective);
        Ok(RV::Scalar(condition))
    }

    pub(super) fn iterator_value(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        index: Value,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let subject = self.builder.ins().load(types::I64, flags(), cursor, 0);
        let cursor_position = if matches!(
            iterator_ty.kind,
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
        ) {
            self.builder.ins().uextend(types::I64, index)
        } else {
            self.builder.ins().load(types::I64, flags(), cursor, 8)
        };
        match iterator_ty.kind {
            l::ForOfKind::ArrayKeys | l::ForOfKind::ArrayKeysReverse => Ok(RV::Scalar(
                self.builder.ins().ireduce(types::I32, cursor_position),
            )),
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayValuesReverse => {
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, subject], false)?
                    .ok_or_else(|| internal("array iterator data has no result"))?;
                let stride = self.ml.layouts.stride(&iterator_ty.element)?;
                let offset = self
                    .builder
                    .ins()
                    .imul_imm(cursor_position, i64::from(stride));
                let address = self.builder.ins().iadd(data, offset);
                self.load_data(&iterator_ty.element, address, 0)
            }
            l::ForOfKind::FixedArrayValues => {
                let stride = self.ml.layouts.stride(&iterator_ty.element)?;
                let offset = self
                    .builder
                    .ins()
                    .imul_imm(cursor_position, i64::from(stride));
                let address = self.builder.ins().iadd(subject, offset);
                self.load_data(&iterator_ty.element, address, 0)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let (size, align) = self.ml.layouts.size_align(&iterator_ty.element)?;
                let output = self.stack_slot(size.max(1), align.max(1));
                let select = self.iconst(
                    types::I32,
                    i64::from(iterator_ty.kind == l::ForOfKind::MapValues),
                );
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.assoc_iter_copy,
                    &[self.ctx, subject, cursor_position, select, output, position],
                    true,
                )?;
                self.load_data(&iterator_ty.element, output, 0)
            }
            l::ForOfKind::StringCodePoints => {
                let current = self.builder.ins().ireduce(types::I32, cursor_position);
                let next = self.stack_slot(4, 4);
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let value = self
                    .call_runtime(
                        self.ml.rt.str_iter_code_point,
                        &[self.ctx, subject, current, next, position],
                        true,
                    )?
                    .ok_or_else(|| internal("string iterator value is missing"))?;
                Ok(RV::Scalar(value))
            }
        }
    }

    pub(super) fn iterator_advance(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        bound: Value,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let next_cursor = self.stack_slot(32, 8);
        self.copy_bytes(next_cursor, cursor, 32, 8);
        let current = self.builder.ins().load(types::I64, flags(), cursor, 8);
        let effective = self.iterator_effective_bound(cursor, iterator_ty, bound, pos)?;
        let subject = self.builder.ins().load(types::I64, flags(), cursor, 0);
        let next = match iterator_ty.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::FixedArrayValues => self.builder.ins().iadd_imm(current, 1),
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse => {
                let no_prior = self.builder.ins().icmp_imm(IntCC::Equal, current, 0);
                let empty = self.builder.ins().icmp_imm(IntCC::Equal, effective, 0);
                let exhausted = self.builder.ins().bor(no_prior, empty);
                let prior = self.builder.ins().iadd_imm(current, -1);
                let last_live = self.builder.ins().iadd_imm(effective, -1);
                let clamped = self.builder.ins().umin(prior, last_live);
                let sentinel = self.iconst(types::I64, -1);
                self.builder.ins().select(exhausted, sentinel, clamped)
            }
            l::ForOfKind::StringCodePoints => {
                let current = self.builder.ins().ireduce(types::I32, current);
                let next = self.stack_slot(4, 4);
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.str_iter_code_point,
                    &[self.ctx, subject, current, next, position],
                    true,
                )?;
                let next = self.builder.ins().load(types::I32, flags(), next, 0);
                self.builder.ins().uextend(types::I64, next)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let start = self.builder.ins().iadd_imm(current, 1);
                self.next_live_assoc_position(subject, iterator_ty, start, effective, pos)?
            }
        };
        self.builder.ins().store(flags(), next, next_cursor, 8);
        Ok(RV::Aggregate(next_cursor))
    }
}

#[cfg(test)]
mod tests {
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{default_libcall_names, FuncOrDataId, Module};
    use subscript_compiler::{check_program, lir as l, SourceFile};

    use crate::lower::func::take_defined_function_texts;
    use crate::lower::{dev_flags, lower_lir_module_with, LowerOptions};

    #[test]
    fn array_iterator_bounds_read_the_header_and_string_bounds_call_runtime() {
        // Lower two small modules; no native execution or external compiler is required.
        for (source, kinds, runtime_name, expected_call) in [
            (
                r#"
export function main(): void {}
function probe(values: i32[]): i32 {
  let total: i32 = 0;
  for (const value of values) { total += value; }
  for (const key of values.keys()) { total += key; }
  return values.reduceRight(
    (acc: i32, value: i32, index: i32): i32 => acc + value + index, total);
}
"#,
                vec![
                    l::ForOfKind::ArrayValues,
                    l::ForOfKind::ArrayKeys,
                    l::ForOfKind::ArrayValuesReverse,
                    l::ForOfKind::ArrayKeysReverse,
                ],
                "subscript_rt_array_len",
                false,
            ),
            (
                r#"
export function main(): void {}
function probe(value: string): i32 {
  let count: i32 = 0;
  for (const point of value) { count += 1; }
  return count;
}
"#,
                vec![l::ForOfKind::StringCodePoints],
                "subscript_rt_str_len",
                true,
            ),
        ] {
            let hir = check_program(&[SourceFile::new("iterator-bound.ts", source)])
                .expect("iterator probe checks");
            let lir = crate::lir::lower_module(&hir).expect("iterator probe LIR");
            for kind in kinds {
                assert!(lir.functions.iter().any(|function| {
                    function.blocks.iter().flat_map(|block| &block.instructions).any(
                        |instruction| matches!(instruction.kind,
                            l::InstructionKind::IteratorCreate { kind: actual, .. } if actual == kind),
                    )
                }), "missing iterator {kind:?}");
            }
            let isa = cranelift_native::builder()
                .expect("host ISA")
                .finish(dev_flags().expect("dev flags"))
                .expect("ISA flags");
            let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
            let _ = take_defined_function_texts();
            lower_lir_module_with(&mut module, &lir, LowerOptions::default())
                .expect("iterator probe JIT lowering");
            let functions = take_defined_function_texts();
            let Some(FuncOrDataId::Func(id)) = module.declarations().get_name(runtime_name) else {
                panic!("missing runtime declaration {runtime_name}");
            };
            let external = format!("u0:{} ", id.as_u32());
            let has_call = functions.iter().any(|(_, text)| {
                text.lines().any(|line| {
                    let Some((reference, declaration)) = line.trim().split_once(" = ") else {
                        return false;
                    };
                    declaration.starts_with(&external)
                        && text.contains(&format!("call {reference}("))
                })
            });
            // SAFETY: no finalized function address escapes this test.
            unsafe { module.free_memory() };
            assert_eq!(has_call, expected_call, "{runtime_name}: {functions:#?}");
        }
    }
}
