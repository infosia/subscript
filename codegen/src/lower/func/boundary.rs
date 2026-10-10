//! Foreign calls and the boundary-struct marshalling.

use super::abi::{
    ensure_sysv_argument_register_capacity, is_pure_hfa_leaves, plan_aggregate_arg_for_signature,
    plan_sysv_struct_return,
};
use super::*;
use subscript_boundary::{CopyBack, PointerPass, StructPass};
use subscript_compiler::CallbackLifetime;

/// The layout question that a boundary C layout answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CLayout {
    /// A struct that the call builds or copies (the write direction).
    Write,
    /// A result that C produces (§187 rule 12).
    Read,
}

/// The error of a fixed array of structs in a struct that a call rebuilds
/// (187.3 item 6). Both tiers stop with this text: no call lowering builds
/// the member, so neither tier copies it back. A fixed array of scalars
/// copies as bytes (§187 rule 13).
pub(crate) fn fixed_array_member(owner: &str, member: &str) -> String {
    internal(format!(
        "boundary field `{owner}.{member}` is a fixed array of structs in a struct that the \
         call rebuilds; no call lowering builds it"
    ))
}

/// True when `ty` is a fixed array of scalars, which a call copies as bytes
/// in a scratch struct (§187 rule 13).
pub(crate) fn is_scalar_fixed_array(module: &l::Module, ty: &Type) -> bool {
    matches!(ty, Type::FixedArray(..))
        && subscript_compiler::boundary_pass::field_shape(module, ty)
            == subscript_boundary::FieldShape::Bytes
}

/// The error of a pair whose elements the call writes back. The binder and
/// the checker reject each position that passes one (§187 rule 11), so
/// this is an internal error in both tiers.
pub(crate) fn written_back_elements(owner: &str) -> String {
    internal(format!(
        "boundary elements of `{owner}` are written back one by one; no call builds them"
    ))
}

/// The error of a written-back target that the build of a pair element
/// reaches. The binder and the checker reject such a pair (§187 rule 11),
/// so this is an internal error in both tiers.
pub(crate) fn written_back_in_elements(target: &str) -> String {
    internal(format!(
        "boundary target `{target}` is written back inside a pair element; no call builds it"
    ))
}

/// The error of a struct cycle that reached code generation. The binder and
/// the checker reject each position that passes one (§187 rule 9), so this
/// is an internal error in both tiers.
pub(crate) fn struct_cycle(owner: &str) -> String {
    internal(format!(
        "boundary struct `{owner}` reaches itself through scratch copies; no call can build it"
    ))
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn foreign_call(
        &mut self,
        id: l::ForeignFunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_info: (Option<&l::ValueType>, Option<Value>),
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let (return_type, endpoint) = return_info;
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
            .filter(|parameter| {
                parameter.foreign_provenance != Some(l::ForeignTypeProvenance::CompletionEndpoint)
            })
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
        if let Some(StructRet::Sret(slot)) = &struct_return {
            arguments.push(*slot);
        }
        // §187 rule 7: the scratch scope opens only for a call whose build
        // can allocate.
        let needs_scratch_scope = declaration
            .parameters
            .iter()
            .any(|parameter| boundary_type_builds_scratch(self.ml.lir, &parameter.ty));
        let scratch_mark = if needs_scratch_scope {
            Some(
                self.call_runtime(self.ml.rt.boundary_scratch_mark, &[self.ctx], false)?
                    .ok_or_else(|| internal("boundary scratch mark has no result"))?,
            )
        } else {
            None
        };
        self.boundary_targets = Some(BoundaryTargets {
            start: self
                .builder
                .current_block()
                .ok_or_else(|| internal("a foreign call has no block"))?,
            slots: Vec::new(),
            elements: 0,
        });
        let mut writebacks = Vec::new();
        let mut cursor = 0usize;
        for parameter in &declaration.parameters {
            if parameter.foreign_provenance == Some(l::ForeignTypeProvenance::CompletionEndpoint) {
                let address = endpoint.ok_or_else(|| internal("completion endpoint is missing"))?;
                self.push_boundary_aggregate(
                    &mut signature,
                    &mut arguments,
                    address,
                    16,
                    8,
                    &BoundaryLeaves {
                        leaves: vec![(0, types::I64), (8, types::I64)],
                    },
                )?;
                continue;
            }
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
                pos,
            )?;
        }
        match return_repr {
            Repr::None | Repr::Agg { .. } => {}
            Repr::Scalar(repr) => signature.returns.push(AbiParam::new(
                return_ty
                    .and_then(crate::layout::native_boundary_type)
                    .unwrap_or(repr),
            )),
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
        let targets = self
            .boundary_targets
            .take()
            .ok_or_else(|| internal("a foreign call lost its targets"))?;
        for writeback in writebacks {
            self.write_back_boundary_pointer(writeback, pos)?;
        }
        self.write_back_boundary_targets(&targets, pos)?;
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
                let mut result = *results
                    .first()
                    .ok_or_else(|| internal("foreign scalar call has no result"))?;
                if return_ty.is_some_and(|ty| {
                    subscript_compiler::types::boundary_kind(ty)
                        .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half)
                }) {
                    result = self.builder.ins().bitcast(
                        types::I16,
                        cranelift_codegen::ir::MemFlags::new(),
                        result,
                    );
                }
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

    /// The LIR class view that the pass decision reads (§187 rule 3).
    fn boundary_view(&self) -> subscript_compiler::boundary_pass::Classes<'_, l::Module> {
        crate::lir::boundary_view(self.ml.lir)
    }

    /// True when the call passes the elements of a pair of `element` as a
    /// scratch array.
    fn elements_need_scratch(&self, element: &Type) -> bool {
        match element {
            Type::Class(class) if self.is_value_class(element) => {
                subscript_boundary::struct_pass(&self.boundary_view(), *class) != StructPass::Bytes
            }
            _ => false,
        }
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
        self.boundary_c_field_in(ty, CLayout::Write)
    }

    /// The C size and alignment of a boundary member type, for a struct
    /// that the call builds or copies (`Write`) or a result that C
    /// produces (`Read`).
    fn boundary_c_field_in(&self, ty: &Type, mode: CLayout) -> Result<(u32, u32), String> {
        if let Some(kind) = subscript_compiler::types::boundary_kind(ty) {
            return Ok((kind.size, kind.align));
        }
        Ok(match ty {
            Type::Func(_) | Type::Object | Type::Nullable(_) => (8, 8),
            Type::Str | Type::Array(_) => (16, 8),
            // A fixed array in a struct that copies its bytes (§187 rule
            // 10) or in a result (rule 12); `boundary_c_layout_in` rejects
            // one in a scratch struct.
            Type::FixedArray(element, length) => {
                let (size, align) = self.boundary_c_field_in(element, mode)?;
                let size = size
                    .checked_mul(*length)
                    .ok_or_else(|| internal("boundary fixed array size overflow"))?;
                (size, align)
            }
            Type::Class(id) if self.is_value_class(ty) => {
                let (_, size, align) = self.boundary_c_layout_in(id.0, mode)?;
                (size, align)
            }
            Type::Class(_) | Type::Map(..) | Type::Set(_) => (8, 8),
            other => return Err(internal(format!("boundary C field type {other:?}"))),
        })
    }

    fn boundary_c_layout(&self, class: usize) -> Result<(Vec<u32>, u32, u32), String> {
        self.boundary_c_layout_in(class, CLayout::Write)
    }

    /// The C offsets, size, and alignment of a boundary class. A `Write`
    /// layout rejects a fixed array in a struct that the call rebuilds; a
    /// `Read` layout is the layout of a result, which no call rebuilds
    /// (§187 rule 12).
    fn boundary_c_layout_in(
        &self,
        class: usize,
        mode: CLayout,
    ) -> Result<(Vec<u32>, u32, u32), String> {
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
            if matches!(field.ty, Type::FixedArray(..))
                && !is_scalar_fixed_array(self.ml.lir, &field.ty)
                && mode == CLayout::Write
                && subscript_boundary::struct_pass(&self.boundary_view(), ClassId(class))
                    != StructPass::Bytes
            {
                return Err(fixed_array_member(
                    &definition.source_name,
                    &field.source_name,
                ));
            }
            let (field_size, field_align) = self.boundary_c_field_in(&field.ty, mode)?;
            size = round_up_layout(size, field_align, "boundary C struct layout")?;
            offsets.push(size);
            size = checked_layout_add(size, field_size, "boundary C struct layout")?;
            align = align.max(field_align);
        }
        size = round_up_layout(size.max(1), align, "final boundary C struct layout")?;
        Ok((offsets, size, align))
    }

    /// The native C-layout leaves of a boundary class.
    /// The walk is total over every field type
    /// `boundary_c_field` sizes, so a leaf list is never partial: an
    /// absorbed callback is one pointer leaf, and a string or array
    /// descriptor is two.
    fn boundary_leaf_components(&self, class: usize) -> Result<BoundaryLeaves, String> {
        self.boundary_leaf_components_in(class, CLayout::Write)
    }

    /// The leaves of [`Self::boundary_leaf_components`] in layout `mode`.
    fn boundary_leaf_components_in(
        &self,
        class: usize,
        mode: CLayout,
    ) -> Result<BoundaryLeaves, String> {
        fn collect<M: Module>(
            body: &Body<'_, '_, '_, '_, M>,
            class: usize,
            base: u32,
            mode: CLayout,
            leaves: &mut Vec<(u32, types::Type)>,
        ) -> Result<(), String> {
            let definition = body
                .ml
                .lir
                .classes
                .get(class)
                .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
            let (offsets, _, _) = body.boundary_c_layout_in(class, mode)?;
            for (field, offset) in definition.fields.iter().zip(offsets) {
                let offset = checked_layout_add(base, offset, "boundary leaf offset")?;
                collect_field(body, &field.ty, offset, mode, leaves)?;
            }
            Ok(())
        }

        fn collect_field<M: Module>(
            body: &Body<'_, '_, '_, '_, M>,
            ty: &Type,
            offset: u32,
            mode: CLayout,
            leaves: &mut Vec<(u32, types::Type)>,
        ) -> Result<(), String> {
            match ty {
                // §187 rule 10: each element of a fixed array in a
                // struct that copies its bytes is a leaf.
                Type::FixedArray(element, length) => {
                    let (stride, _) = body.boundary_c_field_in(element, mode)?;
                    for item in 0..*length {
                        let at = checked_layout_add(
                            offset,
                            stride.checked_mul(item).ok_or_else(|| {
                                internal("boundary fixed array leaf offset overflow")
                            })?,
                            "boundary leaf offset",
                        )?;
                        collect_field(body, element, at, mode, leaves)?;
                    }
                }
                Type::Class(inner) if body.is_value_class(ty) => {
                    collect(body, inner.0, offset, mode, leaves)?;
                }
                Type::Str | Type::Array(_) => {
                    leaves.push((offset, types::I64));
                    let second = checked_layout_add(offset, 8, "boundary leaf offset")?;
                    leaves.push((second, types::I64));
                }
                ty if crate::layout::native_boundary_type(ty).is_some() => {
                    let native = crate::layout::native_boundary_type(ty)
                        .ok_or_else(|| internal("boundary leaf kind is missing"))?;
                    leaves.push((offset, native));
                }
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
            Ok(())
        }

        let mut leaves = Vec::new();
        collect(self, class, 0, mode, &mut leaves)?;
        Ok(BoundaryLeaves { leaves })
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
        match plan_aggregate_arg_for_signature(abi, &components.leaves, size, signature)? {
            AggregateArgPlan::Hfa(hfa) => {
                for (offset, ty) in hfa {
                    let value = self.builder.ins().load(ty, flags(), address, offset as i32);
                    self.push_foreign_argument(signature, arguments, ty, value);
                }
            }
            AggregateArgPlan::StackImages {
                padding,
                padding_type,
                images,
            } => {
                for _ in 0..padding {
                    let zero = if padding_type.is_float() {
                        self.builder.ins().f64const(0.0)
                    } else {
                        self.builder.ins().iconst(padding_type, 0)
                    };
                    self.push_foreign_argument(signature, arguments, padding_type, zero);
                }
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
            AggregateArgPlan::Images(images) => {
                if abi == AggregateAbi::SysV {
                    ensure_sysv_argument_register_capacity(signature, &images)?;
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
        // §187 rule 12: a result lowers by its read facts. No call rebuilds
        // it, so its C layout is the layout of its members as C wrote them.
        let components = self.boundary_leaf_components_in(class.0, CLayout::Read)?;
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
            AggregateAbi::Aapcs64 if is_pure_hfa_leaves(&components.leaves) => {
                Some(components.leaves.clone())
            }
            AggregateAbi::Aapcs64 => {
                (size <= 16).then(|| (0..size.div_ceil(8)).map(|i| (i * 8, types::I64)).collect())
            }
            AggregateAbi::Win64 => match size {
                1 => Some(vec![(0, types::I8)]),
                2 => Some(vec![(0, types::I16)]),
                4 => Some(vec![(0, types::I32)]),
                8 => Some(vec![(0, types::I64)]),
                _ => None,
            },
            AggregateAbi::SysV => {
                plan_sysv_struct_return(&components.leaves, size)?.map(|images| {
                    images
                        .into_iter()
                        .map(|image| (image.offset, image.ty))
                        .collect()
                })
            }
        };
        if let Some(images) = registers {
            for (_, ty) in &images {
                signature.returns.push(AbiParam::new(*ty));
            }
            let slot_size = round_up_layout(size, 8, "struct-return slot")?;
            let slot = self.stack_slot(slot_size, align.max(8));
            self.zero_bytes(slot, slot_size, align.max(8));
            Ok(StructRet::Registers { slot, images })
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
            StructRet::Registers { slot, images } => {
                if results.len() != images.len() {
                    return Err(internal("foreign struct-return register count mismatch"));
                }
                for ((offset, _), value) in images.iter().zip(results) {
                    self.builder
                        .ins()
                        .store(flags(), *value, slot, *offset as i32);
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
                    Type::Class(class) if self.elements_need_scratch(element) => {
                        // §187 rule 7: the call does not write `const`
                        // elements back.
                        let writable = !matches!(
                            &parameter.foreign_provenance,
                            Some(
                                l::ForeignTypeProvenance::Descriptor {
                                    element_const: true,
                                    ..
                                } | l::ForeignTypeProvenance::ScalarPair {
                                    element_const: true,
                                    ..
                                }
                            )
                        );
                        let pass = subscript_boundary::element_pass(
                            &self.boundary_view(),
                            *class,
                            writable,
                        );
                        self.marshal_boundary_array(class.0, pass, data, length, pos)?
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
                self.marshal_boundary_struct(class.0, address, signature, arguments, pos)
            }
            ty if self.boundary_pointer_class(ty).is_some() => {
                let source = self.boundary_pointer_value(value)?;
                let class = self
                    .boundary_pointer_class(ty)
                    .ok_or_else(|| internal("boundary pointer class is missing"))?;
                // §187 rule 7: the call does not write a `const` target
                // back.
                let writable =
                    parameter.foreign_provenance != Some(l::ForeignTypeProvenance::ConstPointer);
                let pass = subscript_boundary::parameter_pass(
                    &self.boundary_view(),
                    ClassId(class),
                    writable,
                );
                match pass {
                    PointerPass::ScriptMemory => {
                        self.push_foreign_argument(signature, arguments, types::I64, source);
                    }
                    PointerPass::ScratchWrittenBack => {
                        let (pointer, writeback) =
                            self.marshal_boundary_pointer(class, source, true, pos)?;
                        self.push_foreign_argument(signature, arguments, types::I64, pointer);
                        writebacks.extend(writeback);
                    }
                    PointerPass::ScratchReadOnly => {
                        let (pointer, _) =
                            self.marshal_boundary_pointer(class, source, false, pos)?;
                        self.push_foreign_argument(signature, arguments, types::I64, pointer);
                    }
                    PointerPass::Cycle => {
                        return Err(struct_cycle(&self.ml.lir.classes[class].source_name));
                    }
                }
                Ok(())
            }
            ty => match self.ml.layouts.repr(ty)? {
                Repr::None => Ok(()),
                Repr::Scalar(repr) => {
                    let mut value = self.expect_scalar(value)?;
                    let native = crate::layout::native_boundary_type(ty).unwrap_or(repr);
                    if subscript_compiler::types::boundary_kind(ty)
                        .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half)
                    {
                        value = self.builder.ins().bitcast(
                            native,
                            cranelift_codegen::ir::MemFlags::new(),
                            value,
                        );
                    }
                    let parameter = AbiParam::new(native);
                    let parameter = match subscript_compiler::types::boundary_kind(ty)
                        .map(|kind| kind.extension)
                    {
                        Some(subscript_boundary::Extension::Signed) => parameter.sext(),
                        Some(subscript_boundary::Extension::Unsigned) => parameter.uext(),
                        _ => parameter,
                    };
                    signature.params.push(parameter);
                    arguments.push(value);
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

    /// Passes the target of a struct-pointer parameter as a scratch copy.
    /// When `written_back` is true, the writeback carries a snapshot of the
    /// bytes that the call put in the scratch struct (§187 rule 7).
    fn marshal_boundary_pointer(
        &mut self,
        class: usize,
        source: Value,
        written_back: bool,
        pos: &Pos,
    ) -> Result<(Value, Option<BoundaryPtrWriteback>), String> {
        let (_, size, align) = self.boundary_c_layout(class)?;
        let scratch = self.stack_slot(size, align);
        self.zero_bytes(scratch, size, align);
        let snapshot = if written_back {
            Some(self.stack_slot(size, align))
        } else {
            None
        };
        let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, source, 0);
        let populate = self.builder.create_block();
        let ready = self.builder.create_block();
        self.builder.ins().brif(nonnull, populate, &[], ready, &[]);
        self.builder.switch_to_block(populate);
        self.populate_boundary_value(class, source, scratch, pos)?;
        if let Some(snapshot) = snapshot {
            self.copy_bytes(snapshot, scratch, size, align);
        }
        self.builder.ins().jump(ready, &[]);
        self.builder.switch_to_block(ready);
        let null = self.iconst(types::I64, 0);
        let pointer = self.builder.ins().select(nonnull, scratch, null);
        Ok((
            pointer,
            snapshot.map(|snapshot| BoundaryPtrWriteback {
                class,
                source,
                scratch,
                snapshot,
            }),
        ))
    }

    fn populate_boundary_value(
        &mut self,
        class: usize,
        source: Value,
        destination: Value,
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
                    // §111 rule 2: the lowering reads the lifetime off the
                    // class. It never derives it from the source name.
                    let explicit = definition.callback_lifetime == CallbackLifetime::Explicit;
                    let trampoline_id = if explicit {
                        self.ml.rt.cb_registration_trampoline
                    } else {
                        self.ml.rt.cb_trampoline
                    };
                    let trampoline = self
                        .ml
                        .module
                        .declare_func_in_func(trampoline_id, self.builder.func);
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
                    // §111 rule 4: an explicit-lifetime crossing creates one
                    // registration; every other crossing binds as before.
                    let crossing = if explicit {
                        self.ml.rt.cb_register
                    } else {
                        self.ml.rt.cb_bind
                    };
                    let binding = self
                        .call_runtime(
                            crossing,
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
                        Type::Class(element_class) if self.elements_need_scratch(element) => {
                            let writable = field.foreign_provenance
                                != Some(l::ForeignTypeProvenance::ConstPointer);
                            let pass = subscript_boundary::element_pass(
                                &self.boundary_view(),
                                *element_class,
                                writable,
                            );
                            self.marshal_boundary_array(
                                element_class.0,
                                pass,
                                source_data,
                                length,
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
                    // The populated struct is a scratch struct. The copy-back
                    // keeps the script link, and the call writes back the
                    // scratch copy of a non-`const` target (§187 rule 7).
                    let writable =
                        field.foreign_provenance != Some(l::ForeignTypeProvenance::ConstPointer);
                    let target = match subscript_boundary::member_pass(
                        &self.boundary_view(),
                        StructPass::Scratch,
                        ClassId(child_class),
                        writable,
                    ) {
                        PointerPass::ScriptMemory => source_pointer,
                        PointerPass::ScratchWrittenBack => {
                            self.scratch_target(child_class, source_pointer, true, pos)?
                        }
                        PointerPass::ScratchReadOnly => {
                            self.scratch_target(child_class, source_pointer, false, pos)?
                        }
                        PointerPass::Cycle => {
                            return Err(struct_cycle(
                                &self.ml.lir.classes[child_class].source_name,
                            ));
                        }
                    };
                    self.builder
                        .ins()
                        .store(flags(), target, destination, c_offset);
                    index += 1;
                }
                Type::Class(inner) if self.is_value_class(&field.ty) => {
                    let source = self.address_offset(source, i64::from(language_offset));
                    let destination = self.address_offset(destination, i64::from(c_offset));
                    match subscript_boundary::embedded_pass(
                        &self.boundary_view(),
                        StructPass::Scratch,
                        *inner,
                    ) {
                        StructPass::Scratch => {
                            self.populate_boundary_value(inner.0, source, destination, pos)?
                        }
                        StructPass::Bytes => {
                            let layout = self.ml.layouts.class(inner.0)?.clone();
                            self.copy_bytes(destination, source, layout.size, layout.align);
                        }
                        StructPass::Cycle => {
                            return Err(struct_cycle(&self.ml.lir.classes[inner.0].source_name));
                        }
                    }
                    index += 1;
                }
                // §187 rule 13: a fixed array of scalars copies as bytes.
                ty @ Type::FixedArray(..) => {
                    if !is_scalar_fixed_array(self.ml.lir, ty) {
                        return Err(fixed_array_member(
                            &definition.source_name,
                            &field.source_name,
                        ));
                    }
                    let (size, align) = self.boundary_c_field(ty)?;
                    let from = self.address_offset(source, i64::from(language_offset));
                    let to = self.address_offset(destination, i64::from(c_offset));
                    self.copy_bytes(to, from, size, align);
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

    /// The scratch copy of the script struct of class `class` at `source`,
    /// or null for a null `source`. A written-back copy holds its snapshot
    /// after the scratch struct, and stores `source` and the copy in a
    /// stack slot of the call, which the call writes back (§187 rule 7).
    fn scratch_target(
        &mut self,
        class: usize,
        source: Value,
        written_back: bool,
        pos: &Pos,
    ) -> Result<Value, String> {
        let slot = if written_back {
            let targets = self
                .boundary_targets
                .as_mut()
                .ok_or_else(|| internal("a written-back target has no call"))?;
            if targets.elements != 0 {
                return Err(written_back_in_elements(
                    &self.ml.lir.classes[class].source_name,
                ));
            }
            let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                16,
                3,
            ));
            self.boundary_targets
                .as_mut()
                .ok_or_else(|| internal("a written-back target has no call"))?
                .slots
                .push((class, slot));
            Some(slot)
        } else {
            None
        };
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let populate = self.builder.create_block();
        let null = self.iconst(types::I64, 0);
        let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, source, 0);
        self.builder
            .ins()
            .brif(nonnull, populate, &[], done, &[BlockArg::Value(null)]);
        self.builder.switch_to_block(populate);
        let (_, size, align) = self.boundary_c_layout(class)?;
        let copies = if written_back { 2 } else { 1 };
        let bytes = self.iconst(types::I64, i64::from(size) * copies);
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        let scratch = self
            .call_runtime(
                self.ml.rt.boundary_scratch_alloc,
                &[self.ctx, bytes, position],
                false,
            )?
            .ok_or_else(|| internal("boundary scratch target is missing"))?;
        self.trap_check();
        self.populate_boundary_value(class, source, scratch, pos)?;
        if let Some(slot) = slot {
            let snapshot = self.address_offset(scratch, i64::from(size));
            self.copy_bytes(snapshot, scratch, size, align);
            self.builder.ins().stack_store(source, slot, 0);
            self.builder.ins().stack_store(scratch, slot, 8);
        }
        self.builder.ins().jump(done, &[BlockArg::Value(scratch)]);
        self.builder.switch_to_block(done);
        Ok(self.builder.block_params(done)[0])
    }

    /// Builds a scratch array of the `length` script elements at `source`,
    /// which the call passes as `pass`. The binder and the checker reject a
    /// pair whose elements the call writes back (§187 rule 11), so `pass`
    /// is [`PointerPass::ScratchReadOnly`].
    fn marshal_boundary_array(
        &mut self,
        element_class: usize,
        pass: PointerPass,
        source: Value,
        length: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        match pass {
            PointerPass::ScratchReadOnly => {}
            PointerPass::ScratchWrittenBack => {
                return Err(written_back_elements(
                    &self.ml.lir.classes[element_class].source_name,
                ))
            }
            PointerPass::Cycle => {
                return Err(struct_cycle(
                    &self.ml.lir.classes[element_class].source_name,
                ))
            }
            PointerPass::ScriptMemory => {
                return Err(internal(
                    "the elements of a pair copy their bytes and need no scratch array",
                ))
            }
        }
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
        // §187 rule 11: no build inside the loop writes back a target.
        self.element_depth(1)?;
        let populated =
            self.populate_boundary_value(element_class, source_element, destination_element, pos);
        self.element_depth(-1)?;
        populated?;
        let next = self.builder.ins().iadd_imm(index, 1);
        self.builder.ins().jump(condition, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        Ok(scratch)
    }

    /// Moves the element-loop depth of the call by `step`.
    fn element_depth(&mut self, step: i32) -> Result<(), String> {
        let targets = self
            .boundary_targets
            .as_mut()
            .ok_or_else(|| internal("a scratch array has no call"))?;
        targets.elements = targets
            .elements
            .checked_add_signed(step)
            .ok_or_else(|| internal("element-loop depth"))?;
        Ok(())
    }

    /// Writes back each target of `targets` after the call, the last one
    /// built first. The start of the build nulls each stack slot, so a
    /// target that the build did not reach (a null link) writes nothing.
    fn write_back_boundary_targets(
        &mut self,
        targets: &BoundaryTargets,
        pos: &Pos,
    ) -> Result<(), String> {
        if targets.slots.is_empty() {
            return Ok(());
        }
        {
            use cranelift_codegen::cursor::{Cursor, FuncCursor};
            let mut cursor =
                FuncCursor::new(self.builder.func).at_first_insertion_point(targets.start);
            let null = cursor.ins().iconst(types::I64, 0);
            for (_, slot) in &targets.slots {
                cursor.ins().stack_store(null, *slot, 0);
            }
        }
        for (class, slot) in targets.slots.iter().rev() {
            let (_, size, _) = self.boundary_c_layout(*class)?;
            let source = self.builder.ins().stack_load(types::I64, *slot, 0);
            let scratch = self.builder.ins().stack_load(types::I64, *slot, 8);
            let snapshot = self.address_offset(scratch, i64::from(size));
            self.write_back_boundary_pointer(
                BoundaryPtrWriteback {
                    class: *class,
                    source,
                    scratch,
                    snapshot,
                },
                pos,
            )?;
        }
        Ok(())
    }

    /// Writes back the members of one scratch struct that C changed (§187
    /// rules 6 and 7): each member that [`subscript_boundary::copy_back`]
    /// names is written only when its scratch bytes differ from the
    /// snapshot that the call took after it built the struct. A script
    /// write that a callback makes during the call stays when C leaves the
    /// member unchanged.
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
            let shape = subscript_compiler::boundary_pass::field_shape(self.ml.lir, &field.ty);
            let copy_back = subscript_boundary::copy_back(&shape);
            if copy_back == CopyBack::Skip {
                continue;
            }
            if let CopyBack::Embedded(inner) = copy_back {
                match subscript_boundary::struct_pass(&self.boundary_view(), inner) {
                    StructPass::Scratch => {
                        let scratch = self.address_offset(writeback.scratch, i64::from(c_offset));
                        let snapshot = self.address_offset(writeback.snapshot, i64::from(c_offset));
                        let source =
                            self.address_offset(writeback.source, i64::from(language_offset));
                        self.write_back_boundary_pointer(
                            BoundaryPtrWriteback {
                                class: inner.0,
                                source,
                                scratch,
                                snapshot,
                            },
                            pos,
                        )?;
                        continue;
                    }
                    StructPass::Bytes => {}
                    StructPass::Cycle => {
                        return Err(struct_cycle(&self.ml.lir.classes[inner.0].source_name));
                    }
                }
            }
            let (c_size, _) = self.boundary_c_field(&field.ty)?;
            let changed =
                self.bytes_differ(writeback.scratch, writeback.snapshot, c_offset, c_size);
            let write = self.builder.create_block();
            let kept = self.builder.create_block();
            self.builder.ins().brif(changed, write, &[], kept, &[]);
            self.builder.switch_to_block(write);
            match copy_back {
                CopyBack::Skip => {}
                CopyBack::StringView => {
                    // A view that C left as the call built it keeps the
                    // script string: the copy-back allocates only for a
                    // view that C wrote.
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
                CopyBack::Embedded(inner) => {
                    let layout = self.ml.layouts.class(inner.0)?.clone();
                    let source = self.address_offset(writeback.scratch, i64::from(c_offset));
                    let destination =
                        self.address_offset(writeback.source, i64::from(language_offset));
                    self.copy_bytes(destination, source, layout.size, layout.align);
                }
                CopyBack::Bytes if matches!(field.ty, Type::FixedArray(..)) => {
                    let from = self.address_offset(writeback.scratch, i64::from(c_offset));
                    let to = self.address_offset(writeback.source, i64::from(language_offset));
                    let (_, align) = self.boundary_c_field(&field.ty)?;
                    self.copy_bytes(to, from, c_size, align);
                }
                CopyBack::Bytes => {
                    let ty = &field.ty;
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
            self.builder.ins().jump(kept, &[]);
            self.builder.switch_to_block(kept);
        }
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }

    /// True (an `i8`) when the `size` bytes at `offset` of `left` and of
    /// `right` differ. The compare reads eight, four, two, and one bytes at
    /// a time and needs no alignment.
    fn bytes_differ(&mut self, left: Value, right: Value, offset: i32, size: u32) -> Value {
        let mut differ = self.iconst(types::I64, 0);
        let mut done = 0u32;
        for (width, ty) in [
            (8u32, types::I64),
            (4, types::I32),
            (2, types::I16),
            (1, types::I8),
        ] {
            while size - done >= width {
                let at = offset + done as i32;
                let unaligned = cranelift_codegen::ir::MemFlags::new();
                let a = self.builder.ins().load(ty, unaligned, left, at);
                let b = self.builder.ins().load(ty, unaligned, right, at);
                let mut bits = self.builder.ins().bxor(a, b);
                if ty != types::I64 {
                    bits = self.builder.ins().uextend(types::I64, bits);
                }
                differ = self.builder.ins().bor(differ, bits);
                done += width;
            }
        }
        self.builder.ins().icmp_imm(IntCC::NotEqual, differ, 0)
    }

    fn marshal_boundary_struct(
        &mut self,
        class: usize,
        source: Value,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        let (_, size, align) = self.boundary_c_layout(class)?;
        let components = self.boundary_leaf_components(class)?;
        match subscript_boundary::value_parameter_pass(&self.boundary_view(), ClassId(class)) {
            // §187 rule 10: the C bytes are the script bytes, a fixed array
            // of scalars included.
            StructPass::Bytes => {
                self.push_boundary_aggregate(signature, arguments, source, size, align, &components)
            }
            StructPass::Scratch => {
                let scratch = self.stack_slot(size, align);
                self.zero_bytes(scratch, size, align);
                self.populate_boundary_value(class, source, scratch, pos)?;
                self.push_boundary_aggregate(
                    signature,
                    arguments,
                    scratch,
                    size,
                    align,
                    &components,
                )
            }
            StructPass::Cycle => Err(struct_cycle(&self.ml.lir.classes[class].source_name)),
        }
    }
}
