//! Foreign calls and the boundary-struct marshalling.

use super::abi::{
    ensure_sysv_argument_register_capacity, is_pure_hfa_leaves, plan_aggregate_arg,
    plan_sysv_struct_return,
};
use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn foreign_call(
        &mut self,
        id: l::ForeignFunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let declaration = self
            .ml
            .lir
            .foreign_functions
            .get(id.0 as usize)
            .ok_or_else(|| internal(format!("foreign function {} is missing", id.0)))?
            .clone();
        let operand_count = declaration
            .parameters
            .iter()
            .map(|parameter| usize::from(matches!(parameter.ty, Type::Array(_))) + 1)
            .sum::<usize>();
        if operands.len() != operand_count || parameter_types.len() != operand_count {
            return Err(internal(format!(
                "foreign call `{}` has inconsistent arity",
                declaration.source_name
            )));
        }
        let mut signature = Signature::new(self.ml.call_conv);
        let return_ty = return_type.map(data_type).transpose()?;
        let return_repr = return_ty
            .map(|ty| self.ml.layouts.repr(ty))
            .transpose()?
            .unwrap_or(Repr::None);
        let struct_return = match (return_ty, return_repr) {
            (Some(ty), Repr::Agg { size, align }) => {
                Some(self.plan_foreign_struct_return(ty, size, align, &mut signature, pos)?)
            }
            _ => None,
        };
        let mut arguments = Vec::new();
        if let Some(StructRet::Sret(slot)) = struct_return {
            arguments.push(slot);
        }
        let needs_scratch_scope = declaration.parameters.iter().any(|parameter| {
            matches!(
                parameter.ty,
                Type::Class(_) | Type::Array(_) | Type::Nullable(_)
            )
        });
        let scratch_mark = if needs_scratch_scope {
            Some(
                self.call_runtime(self.ml.rt.boundary_scratch_mark, &[self.ctx], false)?
                    .ok_or_else(|| internal("boundary scratch mark has no result"))?,
            )
        } else {
            None
        };
        let mut writebacks = Vec::new();
        let mut cursor = 0usize;
        for parameter in &declaration.parameters {
            let (value, array_snapshot) = if let Type::Array(element) = &parameter.ty {
                let data_ty = parameter_types
                    .get(cursor)
                    .ok_or_else(|| internal("foreign array data type is missing"))?;
                let count_ty = parameter_types
                    .get(cursor + 1)
                    .ok_or_else(|| internal("foreign array count type is missing"))?;
                let expected_data = l::ValueType::Address(l::AddressType {
                    pointee: (**element).clone(),
                    array_base: None,
                });
                if data_ty != &expected_data || count_ty != &l::ValueType::Data(Type::I32) {
                    return Err(internal(format!(
                        "foreign array parameter `{}` snapshot types disagree with the declaration",
                        parameter.source_name
                    )));
                }
                let data = self.expect_scalar(
                    *operands
                        .get(cursor)
                        .ok_or_else(|| internal("foreign array data is missing"))?,
                )?;
                let count = self.expect_scalar(
                    *operands
                        .get(cursor + 1)
                        .ok_or_else(|| internal("foreign array count is missing"))?,
                )?;
                cursor += 2;
                (RV::None, Some((data, count)))
            } else {
                let ty = parameter_types
                    .get(cursor)
                    .ok_or_else(|| internal("foreign parameter type is missing"))?;
                if !foreign_parameter_type_matches(self.ml.lir, ty, &parameter.ty) {
                    return Err(internal(format!(
                        "foreign parameter `{}` type disagrees with LIR call target",
                        parameter.source_name
                    )));
                }
                let value = *operands
                    .get(cursor)
                    .ok_or_else(|| internal("foreign parameter value is missing"))?;
                cursor += 1;
                (value, None)
            };
            self.marshal_foreign_argument(
                parameter,
                value,
                array_snapshot,
                &mut signature,
                &mut arguments,
                &mut writebacks,
                scratch_mark,
                pos,
            )?;
        }
        match return_repr {
            Repr::None | Repr::Agg { .. } => {}
            Repr::Scalar(repr) => signature.returns.push(AbiParam::new(repr)),
            Repr::Pair => return Err(internal("foreign function returns a function pair")),
        }
        let function =
            if let Some(function) = self.ml.foreign_ids.get(&declaration.source_name).copied() {
                function
            } else {
                let function = self
                    .ml
                    .module
                    .declare_function(&declaration.source_name, Linkage::Import, &signature)
                    .map_err(|error| {
                        internal(format!(
                            "declare foreign `{}`: {error}",
                            declaration.source_name
                        ))
                    })?;
                self.ml
                    .foreign_ids
                    .insert(declaration.source_name.clone(), function);
                self.ml
                    .foreign_symbols
                    .push(declaration.source_name.clone());
                function
            };
        let reference = self
            .ml
            .module
            .declare_func_in_func(function, self.builder.func);
        let call = self.builder.ins().call(reference, &arguments);
        let results = self.builder.inst_results(call).to_vec();
        for trap in traps {
            if trap.kind == l::TrapKind::Call {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        for writeback in writebacks {
            self.write_back_boundary_pointer(writeback, pos)?;
        }
        if let Some(mark) = scratch_mark {
            self.call_runtime(
                self.ml.rt.boundary_scratch_release,
                &[self.ctx, mark],
                false,
            )?;
        }
        Ok(match return_repr {
            Repr::None => RV::None,
            Repr::Scalar(_) => {
                let result = *results
                    .first()
                    .ok_or_else(|| internal("foreign scalar call has no result"))?;
                if let Some(Type::StringAlias(alias)) = return_ty {
                    let trap = traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::WireEnumValue(*alias))
                        .ok_or_else(|| internal("wire-enum foreign return has no trap"))?;
                    self.validate_wire_alias(*alias, result, trap)?;
                }
                RV::Scalar(result)
            }
            Repr::Agg { .. } => RV::Aggregate(self.finish_foreign_struct_return(
                struct_return.ok_or_else(|| internal("foreign struct-return plan is missing"))?,
                &results,
            )?),
            Repr::Pair => unreachable!("rejected above"),
        })
    }

    fn push_foreign_argument(
        &self,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        ty: types::Type,
        value: Value,
    ) {
        signature.params.push(AbiParam::new(ty));
        arguments.push(value);
    }

    fn validate_wire_alias(
        &mut self,
        alias: subscript_compiler::StringAliasId,
        wire: Value,
        trap: &l::Trap,
    ) -> Result<(), String> {
        let values = self
            .ml
            .lir
            .string_aliases
            .get(alias.0)
            .and_then(|definition| definition.wire_values.clone())
            .ok_or_else(|| internal("foreign string alias return has no wire mapping"))?;
        let mut valid = self.iconst(types::I8, 0);
        for value in values {
            let matches = self
                .builder
                .ins()
                .icmp_imm(IntCC::Equal, wire, i64::from(value));
            valid = self.builder.ins().bor(valid, matches);
        }
        self.emit_trap(trap, TrapOperand::WireValue { wire, valid })
    }

    pub(super) fn validate_wire_alias_traps(
        &mut self,
        ty: &Type,
        value: RV,
        traps: &[l::Trap],
    ) -> Result<(), String> {
        for trap in traps {
            let l::TrapKind::WireEnumValue(alias) = trap.kind else {
                continue;
            };
            if ty != &Type::StringAlias(alias) {
                return Err(internal("wire-enum trap disagrees with its value type"));
            }
            self.validate_wire_alias(alias, self.expect_scalar(value)?, trap)?;
        }
        Ok(())
    }

    pub(super) fn is_value_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.ml.layouts.class(id.0).is_ok_and(|layout| layout.is_value))
    }

    fn boundary_pointer_class(&self, ty: &Type) -> Option<usize> {
        boundary_box_class(self.ml.lir, ty).map(|class| class.0)
    }

    fn boundary_pointer_value(&self, value: RV) -> Result<Value, String> {
        match value {
            RV::Aggregate(address) | RV::Scalar(address) => Ok(address),
            other => Err(internal(format!("boundary pointer from {other:?}"))),
        }
    }

    fn boundary_c_field(&self, ty: &Type) -> Result<(u32, u32), String> {
        Ok(match ty {
            Type::Func(_) | Type::Object | Type::Nullable(_) => (8, 8),
            Type::Str | Type::Array(_) => (16, 8),
            Type::I8 | Type::U8 | Type::Bool => (1, 1),
            Type::I16 | Type::U16 | Type::F16 => (2, 2),
            Type::I32 | Type::U32 | Type::F32 | Type::Enum(_) | Type::StringAlias(_) => (4, 4),
            Type::I64 | Type::U64 | Type::F64 => (8, 8),
            Type::Class(id) if self.is_value_class(ty) => {
                let (_, size, align) = self.boundary_c_layout(id.0)?;
                (size, align)
            }
            Type::Class(_) | Type::Map(..) | Type::Set(_) => (8, 8),
            other => return Err(internal(format!("boundary C field type {other:?}"))),
        })
    }

    fn boundary_c_layout(&self, class: usize) -> Result<(Vec<u32>, u32, u32), String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(class)
            .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
        let mut offsets = Vec::with_capacity(definition.fields.len());
        let mut size = 0u32;
        let mut align = 1u32;
        for field in &definition.fields {
            let (field_size, field_align) = self.boundary_c_field(&field.ty)?;
            size = round_up_layout(size, field_align, "boundary C struct layout")?;
            offsets.push(size);
            size = checked_layout_add(size, field_size, "boundary C struct layout")?;
            align = align.max(field_align);
        }
        size = round_up_layout(size.max(1), align, "final boundary C struct layout")?;
        Ok((offsets, size, align))
    }

    /// The C-layout leaves of a boundary class, and the byte offsets of its
    /// `f16` fields. The walk is total over every field type
    /// `boundary_c_field` sizes, so a leaf list is never partial: an
    /// absorbed callback is one pointer leaf, and a string or array
    /// descriptor is two.
    fn boundary_leaf_components(&self, class: usize) -> Result<BoundaryLeaves, String> {
        fn collect<M: Module>(
            body: &Body<'_, '_, '_, '_, M>,
            class: usize,
            base: u32,
            leaves: &mut Vec<(u32, types::Type)>,
            f16_offsets: &mut Vec<u32>,
        ) -> Result<(), String> {
            let definition = body
                .ml
                .lir
                .classes
                .get(class)
                .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
            let (offsets, _, _) = body.boundary_c_layout(class)?;
            for (field, offset) in definition.fields.iter().zip(offsets) {
                let offset = checked_layout_add(base, offset, "boundary leaf offset")?;
                match &field.ty {
                    Type::Class(inner) if body.is_value_class(&field.ty) => {
                        collect(body, inner.0, offset, leaves, f16_offsets)?;
                    }
                    Type::Str | Type::Array(_) => {
                        leaves.push((offset, types::I64));
                        let second = checked_layout_add(offset, 8, "boundary leaf offset")?;
                        leaves.push((second, types::I64));
                    }
                    Type::F16 => {
                        f16_offsets.push(offset);
                        leaves.push((offset, types::I16));
                    }
                    Type::F32 => leaves.push((offset, types::F32)),
                    Type::F64 => leaves.push((offset, types::F64)),
                    Type::Bool | Type::I8 | Type::U8 => leaves.push((offset, types::I8)),
                    Type::I16 | Type::U16 => leaves.push((offset, types::I16)),
                    Type::I32 | Type::U32 | Type::Enum(_) | Type::StringAlias(_) => {
                        leaves.push((offset, types::I32))
                    }
                    Type::I64 | Type::U64 => leaves.push((offset, types::I64)),
                    Type::Func(_)
                    | Type::Object
                    | Type::Nullable(_)
                    | Type::Class(_)
                    | Type::Map(..)
                    | Type::Set(_) => leaves.push((offset, types::I64)),
                    other => {
                        return Err(internal(format!("boundary C field type {other:?}")));
                    }
                }
            }
            Ok(())
        }

        let mut leaves = Vec::new();
        let mut f16_offsets = Vec::new();
        collect(self, class, 0, &mut leaves, &mut f16_offsets)?;
        Ok(BoundaryLeaves {
            leaves,
            f16_offsets,
        })
    }

    /// Passes a by-value boundary aggregate the way the platform C ABI
    /// passes it (`specs/blocks/compiler.md` §12.3a). AAPCS64, Win64, and
    /// x86-64 SysV are implemented; any other dev host fails loud, because
    /// dev-JIT ≡ ship-C is otherwise unverifiable there.
    fn push_boundary_aggregate(
        &mut self,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        address: Value,
        size: u32,
        align: u32,
        components: &BoundaryLeaves,
    ) -> Result<(), String> {
        let triple = self.ml.module.isa().triple().clone();
        let abi = AggregateAbi::of(&triple).ok_or_else(|| {
            internal(format!(
                "foreign call passing a boundary struct by value is supported on aarch64 \
                 (AAPCS64) and on x86-64 (Win64 or SysV) in the dev JIT (compiler.md \
                 §12.3a); target {triple} is unsupported"
            ))
        })?;
        match plan_aggregate_arg(abi, &components.leaves, size)? {
            AggregateArgPlan::Hfa(hfa) => {
                for (offset, ty) in hfa {
                    let value = self.builder.ins().load(ty, flags(), address, offset as i32);
                    self.push_foreign_argument(signature, arguments, ty, value);
                }
            }
            AggregateArgPlan::Images(images) => {
                if abi == AggregateAbi::SysV {
                    ensure_sysv_argument_register_capacity(
                        signature,
                        &images,
                        &components.f16_offsets,
                    )?;
                }
                // Every image is read from a zero-filled copy, so a trailing
                // partial eightbyte carries defined bytes.
                let image_size = round_up_layout(size.max(1), 8, "boundary aggregate image")?;
                let copy = self.stack_slot(image_size, align.max(8));
                self.zero_bytes(copy, image_size, align.max(8));
                self.copy_bytes(copy, address, size, align.max(1));
                for image in images {
                    let value =
                        self.builder
                            .ins()
                            .load(image.ty, flags(), copy, image.offset as i32);
                    self.push_foreign_argument(signature, arguments, image.ty, value);
                }
            }
            AggregateArgPlan::Indirect => {
                let copy = self.stack_slot(size, align);
                self.copy_bytes(copy, address, size, align);
                self.push_foreign_argument(signature, arguments, types::I64, copy);
            }
            AggregateArgPlan::Memory { stack_size } => {
                let copy = self.stack_slot(stack_size, align.max(8));
                self.zero_bytes(copy, stack_size, align.max(8));
                self.copy_bytes(copy, address, size, align.max(1));
                signature.params.push(AbiParam::special(
                    types::I64,
                    ArgumentPurpose::StructArgument(stack_size),
                ));
                arguments.push(copy);
            }
        }
        Ok(())
    }

    fn plan_foreign_struct_return(
        &mut self,
        ty: &Type,
        size: u32,
        align: u32,
        signature: &mut Signature,
        pos: &Pos,
    ) -> Result<StructRet, String> {
        let Type::Class(class) = ty else {
            return Err(internal("foreign aggregate return is not a class"));
        };
        let triple = self.ml.module.isa().triple().clone();
        let abi = AggregateAbi::of(&triple).ok_or_else(|| {
            internal(format!(
                "foreign call returning a boundary struct by value is supported on aarch64 \
                 (AAPCS64) and on x86-64 (Win64 or SysV) in the dev JIT (compiler.md \
                 §12.3a); target {triple} is unsupported at {pos}"
            ))
        })?;
        let components = self.boundary_leaf_components(class.0)?;
        // An HFA return travels in SIMD registers on every supported ABI,
        // and the dev JIT models no float return register.
        if is_pure_hfa_leaves(&components.leaves) {
            return Err(internal(format!(
                "foreign homogeneous floating-point aggregate return is unsupported at {pos}"
            )));
        }
        let definition = self
            .ml
            .lir
            .classes
            .get(class.0)
            .ok_or_else(|| internal(format!("return class {} is missing", class.0)))?;
        if definition
            .fields
            .iter()
            .any(|field| matches!(field.ty, Type::Func(_) | Type::Array(_) | Type::Str))
        {
            return Err(internal(
                "foreign aggregate return contains an absorbed field",
            ));
        }
        let registers = match abi {
            AggregateAbi::Aapcs64 => (size <= 16).then(|| (size.div_ceil(8), types::I64)),
            AggregateAbi::Win64 => match size {
                1 => Some((1, types::I8)),
                2 => Some((1, types::I16)),
                4 => Some((1, types::I32)),
                8 => Some((1, types::I64)),
                _ => None,
            },
            AggregateAbi::SysV => {
                plan_sysv_struct_return(&components.leaves, size, &components.f16_offsets)?
                    .map(|images| (images.len() as u32, types::I64))
            }
        };
        if let Some((count, ty)) = registers {
            for _ in 0..count {
                signature.returns.push(AbiParam::new(ty));
            }
            let image_bytes = checked_layout_mul(count, ty.bytes(), "struct-return image")?;
            let slot_size = round_up_layout(size.max(image_bytes), 8, "struct-return slot")?;
            let slot = self.stack_slot(slot_size, align.max(8));
            Ok(StructRet::Registers { slot, count, ty })
        } else {
            let slot = self.stack_slot(size, align);
            signature
                .params
                .push(AbiParam::special(types::I64, ArgumentPurpose::StructReturn));
            Ok(StructRet::Sret(slot))
        }
    }

    fn finish_foreign_struct_return(
        &mut self,
        plan: StructRet,
        results: &[Value],
    ) -> Result<Value, String> {
        match plan {
            StructRet::Sret(slot) => Ok(slot),
            StructRet::Registers { slot, count, ty } => {
                if results.len() != count as usize {
                    return Err(internal("foreign struct-return register count mismatch"));
                }
                let stride = ty.bytes() as usize;
                for (index, value) in results.iter().enumerate() {
                    self.builder
                        .ins()
                        .store(flags(), *value, slot, (index * stride) as i32);
                }
                Ok(slot)
            }
        }
    }

    fn marshal_foreign_argument(
        &mut self,
        parameter: &l::ForeignParameter,
        value: RV,
        array_snapshot: Option<(Value, Value)>,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        writebacks: &mut Vec<BoundaryPtrWriteback>,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        match &parameter.ty {
            Type::StringAlias(alias) => {
                let definition = self
                    .ml
                    .lir
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| internal("wire alias is missing"))?;
                if definition.wire_values.is_none() {
                    return Err(internal("plain string alias reached a foreign parameter"));
                }
                let value = self.expect_scalar(value)?;
                self.push_foreign_argument(signature, arguments, types::I32, value);
                Ok(())
            }
            Type::Str => {
                let handle = self.expect_scalar(value)?;
                let data = self
                    .call_runtime(self.ml.rt.str_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("foreign string data is missing"))?;
                let length = self
                    .call_runtime(self.ml.rt.str_len, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("foreign string length is missing"))?;
                let length = self.builder.ins().uextend(types::I64, length);
                let slot = self.stack_slot(16, 8);
                self.builder.ins().store(flags(), data, slot, 0);
                self.builder.ins().store(flags(), length, slot, 8);
                self.push_boundary_aggregate(
                    signature,
                    arguments,
                    slot,
                    16,
                    8,
                    &BoundaryLeaves::descriptor(),
                )
            }
            Type::Array(element) => {
                let (data, length) =
                    array_snapshot.ok_or_else(|| internal("foreign array snapshot is missing"))?;
                let count = self.builder.ins().uextend(types::I64, length);
                let data = match &**element {
                    Type::Class(class)
                        if self.is_value_class(element)
                            && boundary_class_requires_build(self.ml.lir, *class)? =>
                    {
                        self.marshal_boundary_array(
                            class.0,
                            data,
                            length,
                            scratch_mark.ok_or_else(|| {
                                internal("recursive boundary array has no scratch scope")
                            })?,
                            pos,
                        )?
                    }
                    _ => data,
                };
                match &parameter.foreign_provenance {
                    Some(l::ForeignTypeProvenance::Descriptor { .. }) => {
                        let slot = self.stack_slot(16, 8);
                        self.builder.ins().store(flags(), data, slot, 0);
                        self.builder.ins().store(flags(), count, slot, 8);
                        self.push_boundary_aggregate(
                            signature,
                            arguments,
                            slot,
                            16,
                            8,
                            &BoundaryLeaves::descriptor(),
                        )
                    }
                    Some(l::ForeignTypeProvenance::ScalarPair { .. }) => {
                        self.push_foreign_argument(signature, arguments, types::I64, count);
                        self.push_foreign_argument(signature, arguments, types::I64, data);
                        Ok(())
                    }
                    provenance => Err(internal(format!(
                        "foreign array parameter `{}` has incompatible provenance {provenance:?}",
                        parameter.source_name
                    ))),
                }
            }
            Type::Class(class) if self.is_value_class(&parameter.ty) => {
                let address = self.expect_aggregate(value)?;
                self.marshal_boundary_struct(
                    class.0,
                    address,
                    signature,
                    arguments,
                    scratch_mark,
                    pos,
                )
            }
            ty if self.boundary_pointer_class(ty).is_some() => {
                let source = self.boundary_pointer_value(value)?;
                let class = self
                    .boundary_pointer_class(ty)
                    .ok_or_else(|| internal("boundary pointer class is missing"))?;
                if !self
                    .ml
                    .lir
                    .classes
                    .get(class)
                    .is_some_and(|class| class.is_embedded_header)
                    && boundary_class_needs_scratch(self.ml.lir, ClassId(class))?
                {
                    let (pointer, writeback) =
                        self.marshal_boundary_pointer(class, source, scratch_mark, pos)?;
                    self.push_foreign_argument(signature, arguments, types::I64, pointer);
                    writebacks.push(writeback);
                } else {
                    self.push_foreign_argument(signature, arguments, types::I64, source);
                }
                Ok(())
            }
            ty => match self.ml.layouts.repr(ty)? {
                Repr::None => Ok(()),
                Repr::Scalar(repr) => {
                    let value = self.expect_scalar(value)?;
                    self.push_foreign_argument(signature, arguments, repr, value);
                    Ok(())
                }
                other => Err(internal(format!(
                    "foreign parameter `{}` has representation {other:?}",
                    parameter.source_name
                ))),
            },
        }
    }

    pub(super) fn stabilize_boundary_return_value(
        &mut self,
        class: usize,
        source: Value,
    ) -> Result<(), String> {
        self.stabilize_boundary_return_value_inner(class, source, &mut HashSet::new())
    }

    fn stabilize_boundary_return_value_inner(
        &mut self,
        class: usize,
        source: Value,
        visiting: &mut HashSet<usize>,
    ) -> Result<(), String> {
        if !visiting.insert(class) {
            return Ok(());
        }
        let definition = self
            .ml
            .lir
            .classes
            .get(class)
            .cloned()
            .ok_or_else(|| internal(format!("boundary return class {class} is missing")))?;
        let layout = self.ml.layouts.class(class)?.clone();
        for (index, field) in definition.fields.iter().enumerate() {
            let offset = *layout
                .field_offsets
                .get(index)
                .ok_or_else(|| internal("boundary return field offset is missing"))?
                as i32;
            if self.boundary_pointer_class(&field.ty).is_some() {
                // The field already owns a Context-managed box. Its payload
                // does not depend on the returning activation.
                continue;
            }
            if let Type::Class(inner) = &field.ty {
                if self.is_value_class(&field.ty) {
                    let nested = self.address_offset(source, i64::from(offset));
                    self.stabilize_boundary_return_value_inner(inner.0, nested, visiting)?;
                    continue;
                }
            }
            if let Type::Array(element) = &field.ty {
                let Type::Class(element_class) = &**element else {
                    continue;
                };
                if !self.is_value_class(element) {
                    continue;
                }
                let handle = self.builder.ins().load(types::I64, flags(), source, offset);
                let length = self
                    .call_runtime(self.ml.rt.array_len, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("boundary return array length has no result"))?;
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("boundary return array data has no result"))?;
                let stride = self.ml.layouts.stride(element)?;
                let condition = self.builder.create_block();
                let body = self.builder.create_block();
                let done = self.builder.create_block();
                self.builder.append_block_param(condition, types::I32);
                let zero = self.iconst(types::I32, 0);
                self.builder.ins().jump(condition, &[BlockArg::Value(zero)]);
                self.builder.switch_to_block(condition);
                let item = self.builder.block_params(condition)[0];
                let more = self.builder.ins().icmp(IntCC::SignedLessThan, item, length);
                self.builder.ins().brif(more, body, &[], done, &[]);
                self.builder.switch_to_block(body);
                let item64 = self.builder.ins().uextend(types::I64, item);
                let byte_offset = self.builder.ins().imul_imm(item64, i64::from(stride));
                let element_address = self.builder.ins().iadd(data, byte_offset);
                self.stabilize_boundary_return_value_inner(
                    element_class.0,
                    element_address,
                    visiting,
                )?;
                let next = self.builder.ins().iadd_imm(item, 1);
                self.builder.ins().jump(condition, &[BlockArg::Value(next)]);
                self.builder.switch_to_block(done);
            }
        }
        visiting.remove(&class);
        Ok(())
    }

    fn marshal_boundary_pointer(
        &mut self,
        class: usize,
        source: Value,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(Value, BoundaryPtrWriteback), String> {
        let (_, size, align) = self.boundary_c_layout(class)?;
        let scratch = self.stack_slot(size, align);
        self.zero_bytes(scratch, size, align);
        let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, source, 0);
        let populate = self.builder.create_block();
        let ready = self.builder.create_block();
        self.builder.ins().brif(nonnull, populate, &[], ready, &[]);
        self.builder.switch_to_block(populate);
        self.populate_boundary_value(class, source, scratch, scratch_mark, pos)?;
        self.builder.ins().jump(ready, &[]);
        self.builder.switch_to_block(ready);
        let null = self.iconst(types::I64, 0);
        let pointer = self.builder.ins().select(nonnull, scratch, null);
        Ok((
            pointer,
            BoundaryPtrWriteback {
                class,
                source,
                scratch,
            },
        ))
    }

    fn populate_boundary_value(
        &mut self,
        class: usize,
        source: Value,
        destination: Value,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(class)
            .cloned()
            .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
        let language_layout = self.ml.layouts.class(class)?.clone();
        let (c_offsets, _, _) = self.boundary_c_layout(class)?;
        let mut index = 0usize;
        while index < definition.fields.len() {
            let field = &definition.fields[index];
            let language_offset = language_layout.field_offsets[index] as i32;
            let c_offset = c_offsets[index] as i32;
            match &field.ty {
                Type::Func(_) => {
                    let code =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    let environment =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset + 8);
                    let trampoline = self
                        .ml
                        .module
                        .declare_func_in_func(self.ml.rt.cb_trampoline, self.builder.func);
                    let trampoline = self.builder.ins().func_addr(types::I64, trampoline);
                    self.builder
                        .ins()
                        .store(flags(), trampoline, destination, c_offset);
                    let first = definition
                        .fields
                        .get(index + 1)
                        .ok_or_else(|| internal("boundary callback has no userdata field"))?;
                    let first_offset = language_layout.field_offsets[index + 1] as i32;
                    let userdata =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, first_offset);
                    let has_second = definition
                        .fields
                        .get(index + 2)
                        .is_some_and(|field| is_userdata_slot(&field.ty));
                    let userdata2 = if has_second {
                        let offset = language_layout.field_offsets[index + 2] as i32;
                        self.builder.ins().load(types::I64, flags(), source, offset)
                    } else {
                        self.iconst(types::I64, 0)
                    };
                    let binding = self
                        .call_runtime(
                            self.ml.rt.cb_bind,
                            &[self.ctx, code, environment, userdata, userdata2],
                            false,
                        )?
                        .ok_or_else(|| internal("callback binding has no result"))?;
                    self.builder.ins().store(
                        flags(),
                        binding,
                        destination,
                        c_offsets[index + 1] as i32,
                    );
                    if has_second {
                        let zero = self.iconst(types::I64, 0);
                        self.builder.ins().store(
                            flags(),
                            zero,
                            destination,
                            c_offsets[index + 2] as i32,
                        );
                        index += 3;
                    } else {
                        let _ = first;
                        index += 2;
                    }
                }
                Type::Str => {
                    let handle =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    let data = self
                        .call_runtime(self.ml.rt.str_data, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary string data is missing"))?;
                    let length = self
                        .call_runtime(self.ml.rt.str_len, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary string length is missing"))?;
                    let length = self.builder.ins().uextend(types::I64, length);
                    self.builder
                        .ins()
                        .store(flags(), data, destination, c_offset);
                    self.builder
                        .ins()
                        .store(flags(), length, destination, c_offset + 8);
                    index += 1;
                }
                Type::Array(element) => {
                    let handle =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    let length = self
                        .call_runtime(self.ml.rt.array_len, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary array length is missing"))?;
                    let count = self.builder.ins().uextend(types::I64, length);
                    let source_data = self
                        .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary array data is missing"))?;
                    let data = match &**element {
                        Type::Class(element_class)
                            if self.is_value_class(element)
                                && boundary_class_requires_build(self.ml.lir, *element_class)? =>
                        {
                            self.marshal_boundary_array(
                                element_class.0,
                                source_data,
                                length,
                                scratch_mark.ok_or_else(|| {
                                    internal("recursive boundary array has no scratch scope")
                                })?,
                                pos,
                            )?
                        }
                        _ => source_data,
                    };
                    self.builder
                        .ins()
                        .store(flags(), count, destination, c_offset);
                    self.builder
                        .ins()
                        .store(flags(), data, destination, c_offset + 8);
                    index += 1;
                }
                ty if self.boundary_pointer_class(ty).is_some() => {
                    let child_class = self
                        .boundary_pointer_class(ty)
                        .ok_or_else(|| internal("boundary child class is missing"))?;
                    let source_pointer =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    if self
                        .ml
                        .lir
                        .classes
                        .get(child_class)
                        .is_some_and(|class| class.is_embedded_header)
                    {
                        self.builder
                            .ins()
                            .store(flags(), source_pointer, destination, c_offset);
                        index += 1;
                        continue;
                    }
                    let zero = self.iconst(types::I64, 0);
                    self.builder
                        .ins()
                        .store(flags(), zero, destination, c_offset);
                    let nonnull = self
                        .builder
                        .ins()
                        .icmp_imm(IntCC::NotEqual, source_pointer, 0);
                    let populate = self.builder.create_block();
                    let ready = self.builder.create_block();
                    self.builder.ins().brif(nonnull, populate, &[], ready, &[]);
                    self.builder.switch_to_block(populate);
                    let (_, child_size, _) = self.boundary_c_layout(child_class)?;
                    let bytes = self.iconst(types::I64, i64::from(child_size));
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    let child = self
                        .call_runtime(
                            self.ml.rt.boundary_scratch_alloc,
                            &[self.ctx, bytes, position],
                            false,
                        )?
                        .ok_or_else(|| internal("boundary child scratch is missing"))?;
                    self.trap_check();
                    self.populate_boundary_value(
                        child_class,
                        source_pointer,
                        child,
                        scratch_mark,
                        pos,
                    )?;
                    self.builder
                        .ins()
                        .store(flags(), child, destination, c_offset);
                    self.builder.ins().jump(ready, &[]);
                    self.builder.switch_to_block(ready);
                    index += 1;
                }
                Type::Class(inner) if self.is_value_class(&field.ty) => {
                    let source = self.address_offset(source, i64::from(language_offset));
                    let destination = self.address_offset(destination, i64::from(c_offset));
                    if boundary_class_requires_build(self.ml.lir, *inner)? {
                        self.populate_boundary_value(
                            inner.0,
                            source,
                            destination,
                            scratch_mark,
                            pos,
                        )?;
                    } else {
                        let layout = self.ml.layouts.class(inner.0)?.clone();
                        self.copy_bytes(destination, source, layout.size, layout.align);
                    }
                    index += 1;
                }
                ty => {
                    let value = self.load_data(ty, source, language_offset)?;
                    let value = self.expect_scalar(value)?;
                    self.builder
                        .ins()
                        .store(flags(), value, destination, c_offset);
                    index += 1;
                }
            }
        }
        Ok(())
    }

    fn marshal_boundary_array(
        &mut self,
        element_class: usize,
        source: Value,
        length: Value,
        _scratch_mark: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        let language_layout = self.ml.layouts.class(element_class)?.clone();
        let (_, c_size, _) = self.boundary_c_layout(element_class)?;
        let length64 = self.builder.ins().uextend(types::I64, length);
        let bytes = self.builder.ins().imul_imm(length64, i64::from(c_size));
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        let scratch = self
            .call_runtime(
                self.ml.rt.boundary_scratch_alloc,
                &[self.ctx, bytes, position],
                false,
            )?
            .ok_or_else(|| internal("boundary array scratch is missing"))?;
        self.trap_check();
        let condition = self.builder.create_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(condition, types::I32);
        let zero = self.iconst(types::I32, 0);
        self.builder.ins().jump(condition, &[BlockArg::Value(zero)]);
        self.builder.switch_to_block(condition);
        let index = self.builder.block_params(condition)[0];
        let more = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThan, index, length);
        self.builder.ins().brif(more, body, &[], done, &[]);
        self.builder.switch_to_block(body);
        let index64 = self.builder.ins().uextend(types::I64, index);
        let source_offset = self
            .builder
            .ins()
            .imul_imm(index64, i64::from(language_layout.size));
        let destination_offset = self.builder.ins().imul_imm(index64, i64::from(c_size));
        let source_element = self.builder.ins().iadd(source, source_offset);
        let destination_element = self.builder.ins().iadd(scratch, destination_offset);
        self.populate_boundary_value(
            element_class,
            source_element,
            destination_element,
            Some(_scratch_mark),
            pos,
        )?;
        let next = self.builder.ins().iadd_imm(index, 1);
        self.builder.ins().jump(condition, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        Ok(scratch)
    }

    fn write_back_boundary_pointer(
        &mut self,
        writeback: BoundaryPtrWriteback,
        pos: &Pos,
    ) -> Result<(), String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(writeback.class)
            .cloned()
            .ok_or_else(|| internal(format!("boundary class {} is missing", writeback.class)))?;
        let language_layout = self.ml.layouts.class(writeback.class)?.clone();
        let (c_offsets, _, _) = self.boundary_c_layout(writeback.class)?;
        let nonnull = self
            .builder
            .ins()
            .icmp_imm(IntCC::NotEqual, writeback.source, 0);
        let copy = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.ins().brif(nonnull, copy, &[], done, &[]);
        self.builder.switch_to_block(copy);
        for (index, field) in definition.fields.iter().enumerate() {
            let language_offset = language_layout.field_offsets[index] as i32;
            let c_offset = c_offsets[index] as i32;
            match &field.ty {
                Type::Str => {
                    let data =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), writeback.scratch, c_offset);
                    let length = self.builder.ins().load(
                        types::I64,
                        flags(),
                        writeback.scratch,
                        c_offset + 8,
                    );
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    let handle = self
                        .call_runtime(
                            self.ml.rt.str_from_view,
                            &[self.ctx, data, length, position],
                            false,
                        )?
                        .ok_or_else(|| internal("boundary string writeback has no result"))?;
                    self.builder
                        .ins()
                        .store(flags(), handle, writeback.source, language_offset);
                    self.trap_check();
                }
                Type::Array(_) | Type::Nullable(_) | Type::Func(_) => {}
                Type::Class(inner) if self.is_value_class(&field.ty) => {
                    if !boundary_class_requires_build(self.ml.lir, *inner)? {
                        let layout = self.ml.layouts.class(inner.0)?.clone();
                        let source = self.address_offset(writeback.scratch, i64::from(c_offset));
                        let destination =
                            self.address_offset(writeback.source, i64::from(language_offset));
                        self.copy_bytes(destination, source, layout.size, layout.align);
                    }
                }
                ty => {
                    let Repr::Scalar(repr) = self.ml.layouts.repr(ty)? else {
                        return Err(internal(format!(
                            "boundary field `{}` cannot be written back",
                            field.source_name
                        )));
                    };
                    let value = self
                        .builder
                        .ins()
                        .load(repr, flags(), writeback.scratch, c_offset);
                    self.store_data(ty, writeback.source, language_offset, RV::Scalar(value))?;
                }
            }
        }
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }

    fn marshal_boundary_struct(
        &mut self,
        class: usize,
        source: Value,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        let (_, size, align) = self.boundary_c_layout(class)?;
        let scratch = self.stack_slot(size, align);
        self.zero_bytes(scratch, size, align);
        self.populate_boundary_value(class, source, scratch, scratch_mark, pos)?;
        let components = self.boundary_leaf_components(class)?;
        self.push_boundary_aggregate(signature, arguments, scratch, size, align, &components)
    }
}
