//! Value operations, literals, and iteration.

use super::*;

impl Interpreter<'_> {
    pub(super) fn unary(
        &self,
        operator: l::UnaryOp,
        value: &Value,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let ty = match result_ty {
            Some(l::ValueType::Data(ty)) => ty,
            _ => return Err(self.invalid(None, "unary instruction has no data result")),
        };
        match operator {
            l::UnaryOp::Not => Ok(Value::Bool(!value.as_bool()?)),
            l::UnaryOp::Neg => match ty {
                Type::F32 => Ok(Value::F32(-(value.as_f64()? as f32))),
                Type::F64 => Ok(Value::F64(-value.as_f64()?)),
                _ => self.integer_result(ty, value.as_i64()?.wrapping_neg() as u64),
            },
            l::UnaryOp::BitNot => self.integer_result(ty, !value.as_u64()?),
        }
    }

    pub(super) fn binary(
        &mut self,
        operator: l::BinaryOp,
        left: &Value,
        right: &Value,
        result_ty: Option<&l::ValueType>,
        function: &l::Function,
        instruction: &l::Instruction,
    ) -> Result<Value, InterpretError> {
        let ty = match result_ty {
            Some(l::ValueType::Data(ty)) => ty,
            _ => {
                return Err(self.invalid(
                    Some(instruction.pos.clone()),
                    "binary instruction has no data result",
                ));
            }
        };
        if matches!(operator, l::BinaryOp::Eq | l::BinaryOp::Ne) {
            let operand_ty = self
                .instruction_operand_type(function, instruction, 0)
                .unwrap_or(ty);
            let equal = self.equal(left, right, operand_ty)?;
            return Ok(Value::Bool(if operator == l::BinaryOp::Eq {
                equal
            } else {
                !equal
            }));
        }
        if matches!(
            operator,
            l::BinaryOp::Lt | l::BinaryOp::Le | l::BinaryOp::Gt | l::BinaryOp::Ge
        ) {
            let order = match (left, right) {
                (Value::F32(a), Value::F32(b)) => {
                    compare_f64(f64::from(*a), f64::from(*b), operator)
                }
                (Value::F64(a), Value::F64(b)) => compare_f64(*a, *b, operator),
                (Value::I(a), Value::I(b)) => compare_i64(*a, *b, operator),
                _ => compare_u64(left.as_u64()?, right.as_u64()?, operator),
            };
            return Ok(Value::Bool(order));
        }
        if matches!(ty, Type::Str) && operator == l::BinaryOp::Add {
            let mut bytes = self.string_bytes(left.as_handle()?)?;
            bytes.extend_from_slice(&self.string_bytes(right.as_handle()?)?);
            return Ok(Value::Handle(self.alloc_string(&bytes, &instruction.pos)?));
        }
        match ty {
            Type::F32 => {
                let a = left.as_f64()? as f32;
                let b = right.as_f64()? as f32;
                Ok(Value::F32(match operator {
                    l::BinaryOp::Add => a + b,
                    l::BinaryOp::Sub => a - b,
                    l::BinaryOp::Mul => a * b,
                    l::BinaryOp::Div => a / b,
                    l::BinaryOp::Rem => {
                        ffi::subscript_rt_fmod(std::ptr::null_mut(), f64::from(a), f64::from(b))
                            as f32
                    }
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            format!("{operator:?} is not f32 arithmetic"),
                        ));
                    }
                }))
            }
            Type::F64 => {
                let a = left.as_f64()?;
                let b = right.as_f64()?;
                Ok(Value::F64(match operator {
                    l::BinaryOp::Add => a + b,
                    l::BinaryOp::Sub => a - b,
                    l::BinaryOp::Mul => a * b,
                    l::BinaryOp::Div => a / b,
                    l::BinaryOp::Rem => ffi::subscript_rt_fmod(std::ptr::null_mut(), a, b),
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            format!("{operator:?} is not f64 arithmetic"),
                        ));
                    }
                }))
            }
            _ => {
                let a = left.as_u64()?;
                let b = right.as_u64()?;
                let bits = integer_bits(ty).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        format!("{ty:?} is not integer"),
                    )
                })?;
                let shift = (b & u64::from(bits - 1)) as u32;
                let value = match operator {
                    l::BinaryOp::Add => a.wrapping_add(b),
                    l::BinaryOp::Sub => a.wrapping_sub(b),
                    l::BinaryOp::Mul => a.wrapping_mul(b),
                    l::BinaryOp::Div if is_signed(ty) => {
                        sign_extend(a, bits).wrapping_div(sign_extend(b, bits)) as u64
                    }
                    l::BinaryOp::Div => a / b,
                    l::BinaryOp::Rem if is_signed(ty) => {
                        sign_extend(a, bits).wrapping_rem(sign_extend(b, bits)) as u64
                    }
                    l::BinaryOp::Rem => a % b,
                    l::BinaryOp::BitAnd => a & b,
                    l::BinaryOp::BitOr => a | b,
                    l::BinaryOp::BitXor => a ^ b,
                    l::BinaryOp::Shl => a.wrapping_shl(shift),
                    l::BinaryOp::Shr if is_signed(ty) => (sign_extend(a, bits) >> shift) as u64,
                    l::BinaryOp::Shr => mask_bits(a, bits) >> shift,
                    l::BinaryOp::UShr => mask_bits(a, bits) >> shift,
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            format!("{operator:?} is not integer arithmetic"),
                        ));
                    }
                };
                self.integer_result(ty, value)
            }
        }
    }

    pub(super) fn equal(
        &self,
        left: &Value,
        right: &Value,
        ty: &Type,
    ) -> Result<bool, InterpretError> {
        Ok(match ty {
            Type::F32 => left.as_f64()? as f32 == right.as_f64()? as f32,
            Type::F64 => left.as_f64()? == right.as_f64()?,
            Type::Str => {
                self.string_bytes(left.as_handle()?)? == self.string_bytes(right.as_handle()?)?
            }
            Type::Bool => left.as_bool()? == right.as_bool()?,
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                match (left, right) {
                    (Value::Blob(a), Value::Blob(b)) => a == b,
                    _ => false,
                }
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => match (left, right) {
                (Value::Blob(a), Value::Blob(b)) => a == b,
                _ => false,
            },
            Type::Null
            | Type::Nullable(_)
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::RegExp
            | Type::Generator(_) => left.as_handle()? == right.as_handle()?,
            _ if is_signed(ty) => left.as_i64()? == right.as_i64()?,
            _ => left.as_u64()? == right.as_u64()?,
        })
    }

    pub(super) fn integer_result(&self, ty: &Type, value: u64) -> Result<Value, InterpretError> {
        let bits =
            integer_bits(ty).ok_or_else(|| self.invalid(None, format!("{ty:?} is not integer")))?;
        let value = mask_bits(value, bits);
        if is_signed(ty) {
            Ok(Value::I(sign_extend(value, bits)))
        } else {
            Ok(Value::U(value))
        }
    }

    fn saturating_float_integer_result(
        &self,
        ty: &Type,
        value: f64,
    ) -> Result<Value, InterpretError> {
        let bits =
            integer_bits(ty).ok_or_else(|| self.invalid(None, format!("{ty:?} is not integer")))?;
        if value.is_nan() {
            return Ok(if is_signed(ty) {
                Value::I(0)
            } else {
                Value::U(0)
            });
        }
        if is_signed(ty) {
            let minimum = -(1_i128 << (bits - 1));
            let maximum = (1_i128 << (bits - 1)) - 1;
            let value = if value <= minimum as f64 {
                minimum
            } else if value >= maximum as f64 {
                maximum
            } else {
                value.trunc() as i128
            };
            Ok(Value::I(value as i64))
        } else {
            let maximum = (1_u128 << bits) - 1;
            let value = if value <= 0.0 {
                0
            } else if value >= maximum as f64 {
                maximum
            } else {
                value.trunc() as u128
            };
            Ok(Value::U(value as u64))
        }
    }

    pub(super) fn convert(
        &self,
        value: &Value,
        result_ty: Option<&l::ValueType>,
        source_ty: Option<&Type>,
        instruction: &l::Instruction,
    ) -> Result<Value, InterpretError> {
        let ty = self.data_result_type(result_ty, instruction)?;
        if let (Type::Class(target), Some(Type::Nullable(source))) = (ty, source_ty) {
            if matches!(source.as_ref(), Type::Class(source) if source == target)
                && self
                    .module
                    .classes
                    .get(target.0)
                    .is_some_and(|class| class.is_value)
            {
                let handle = value.as_handle()?;
                let layout = self.class_layouts.get(target).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        format!("class {:?} has no box layout", target),
                    )
                })?;
                if handle.is_null() {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "null boundary box was narrowed without a guard",
                    ));
                }
                // SAFETY: BoxBoundaryValue allocates this exact class payload.
                let bytes = unsafe { std::slice::from_raw_parts(handle, layout.size) };
                return self.unpack(ty, bytes);
            }
        }
        Ok(match ty {
            Type::F16 => Value::U(ffi::subscript_rt_f16_from_f64(value.as_f64()?) as u64),
            Type::F32 => Value::F32(if matches!(source_ty, Some(Type::F16)) {
                ffi::subscript_rt_f16_to_f64(value.as_u64()? as u16) as f32
            } else {
                value.as_f64()? as f32
            }),
            Type::F64 => Value::F64(if matches!(source_ty, Some(Type::F16)) {
                ffi::subscript_rt_f16_to_f64(value.as_u64()? as u16)
            } else {
                value.as_f64()?
            }),
            Type::Bool => Value::Bool(value.as_bool()?),
            ty if integer_bits(ty).is_some() => {
                let float = match (source_ty, value) {
                    (Some(Type::F16), Value::U(value)) => {
                        Some(ffi::subscript_rt_f16_to_f64(*value as u16))
                    }
                    (Some(Type::F32), Value::F32(value)) => Some(f64::from(*value)),
                    (Some(Type::F64), Value::F64(value)) => Some(*value),
                    _ => None,
                };
                if let Some(value) = float {
                    return self.saturating_float_integer_result(ty, value);
                }
                let raw = value.as_u64()?;
                self.integer_result(ty, raw)?
            }
            Type::Nullable(_) | Type::Object | Type::Class(_) => value.clone(),
            _ => value.clone(),
        })
    }

    pub(super) fn length(&self, value: &Value, ty: Option<&Type>) -> Result<i32, InterpretError> {
        match (value, ty) {
            (Value::Handle(handle), Some(Type::Str)) => Ok(ffi_len_string(&self.context, *handle)),
            (Value::Handle(handle), Some(Type::Array(_))) => {
                // SAFETY: verified dynamic-array operand.
                Ok(unsafe { self.context.array_len(*handle) })
            }
            (Value::Blob(_), Some(Type::FixedArray(_, count))) => Ok(*count as i32),
            (Value::Blob(bytes), _) => Ok(bytes.len() as i32),
            (other, _) => Err(type_error("container", other)),
        }
    }

    pub(super) fn array_literal(
        &mut self,
        ty: &Type,
        elements: &[Value],
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        if let Type::FixedArray(element_ty, count) = ty {
            if elements.len() != *count as usize {
                return Err(self.invalid(
                    Some(pos.clone()),
                    format!(
                        "fixed array has {} elements for declared count {count}",
                        elements.len()
                    ),
                ));
            }
            let element_layout = self.type_layout(element_ty)?;
            let stride = align_up(element_layout.size, element_layout.align);
            let mut bytes = vec![0; stride * *count as usize];
            for (index, element) in elements.iter().enumerate() {
                self.pack_into(element_ty, element, &mut bytes[index * stride..])?;
            }
            return Ok(Value::Blob(bytes));
        }
        let Type::Array(element_ty) = ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "ArrayLiteral result is neither dynamic nor fixed array",
            ));
        };
        let element_layout = self.type_layout(element_ty)?;
        // SAFETY: Context and element size meet the runtime array contract.
        let array = unsafe {
            ffi::subscript_rt_array_new(&mut *self.context, element_layout.size as u64, 0)
        };
        self.check_runtime(pos)?;
        self.root_handle(array);
        for element in elements {
            let bytes = self.pack(element_ty, element)?;
            // SAFETY: `bytes` is exactly one element and the array is live.
            unsafe { ffi::subscript_rt_array_push(&mut *self.context, array, bytes.as_ptr(), 0) };
            self.check_runtime(pos)?;
        }
        Ok(Value::Handle(array))
    }

    pub(super) fn array_with_capacity(
        &mut self,
        ty: &Type,
        capacity: i64,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let Type::Array(element) = ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "ArrayWithCapacity result is not a dynamic array",
            ));
        };
        let capacity = u64::try_from(capacity).map_err(|_| {
            self.invalid(Some(pos.clone()), "ArrayWithCapacity has a negative bound")
        })?;
        let layout = self.type_layout(element)?;
        // SAFETY: Context and the verified array element layout meet the runtime contract.
        let array = unsafe {
            ffi::subscript_rt_array_with_capacity(
                &mut *self.context,
                capacity,
                layout.size as u64,
                0,
            )
        };
        self.check_runtime(pos)?;
        self.root_handle(array);
        Ok(Value::Handle(array))
    }

    pub(super) fn array_spread_literal(
        &mut self,
        ty: &Type,
        parts: &[Option<l::SpreadKind>],
        operands: &[Value],
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let Type::Array(element_ty) = ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "spread literal result is not a dynamic array",
            ));
        };
        let created = self.array_literal(ty, &[], pos)?;
        let Value::Handle(out) = created else {
            return Err(self.invalid(
                Some(pos.clone()),
                "spread literal allocation did not return an array handle",
            ));
        };
        for (part, operand) in parts.iter().zip(operands) {
            match part {
                None => {
                    let bytes = self.pack(element_ty, operand)?;
                    // SAFETY: one correctly packed element.
                    unsafe {
                        ffi::subscript_rt_array_push(&mut *self.context, out, bytes.as_ptr(), 0)
                    };
                }
                Some(l::SpreadKind::Array) => {
                    // SAFETY: verified identical-element array handles.
                    unsafe {
                        ffi::subscript_rt_array_spread_array(
                            &mut *self.context,
                            out,
                            operand.as_handle()?,
                            0,
                        )
                    };
                }
                Some(l::SpreadKind::FixedArray) => {
                    let Value::Blob(bytes) = operand else {
                        return Err(type_error("fixed array", operand));
                    };
                    let width = self
                        .layout_cached(element_ty)
                        .ok_or_else(|| {
                            self.invalid(Some(pos.clone()), "spread element has no layout")
                        })?
                        .size;
                    // SAFETY: fixed blob consists of whole elements.
                    unsafe {
                        ffi::subscript_rt_array_spread_fixed(
                            &mut *self.context,
                            out,
                            bytes.as_ptr(),
                            (bytes.len() / width) as u64,
                            0,
                        )
                    };
                }
                Some(l::SpreadKind::SetValues) => {
                    // SAFETY: runtime association traversal owns order/bound.
                    unsafe {
                        ffi::subscript_rt_array_spread_assoc(
                            &mut *self.context,
                            out,
                            operand.as_handle()?,
                            0,
                        )
                    };
                }
                Some(l::SpreadKind::StringCodePoints) => {
                    // SAFETY: runtime string code-point traversal.
                    unsafe {
                        ffi::subscript_rt_array_spread_string(
                            &mut *self.context,
                            out,
                            operand.as_handle()?,
                            0,
                        )
                    };
                }
            }
            self.check_runtime(pos)?;
        }
        Ok(Value::Handle(out))
    }

    /// Runs `new Set<K>(source)` as one fused runtime traversal
    /// (compiler.md §103.1 rule 4).
    pub(super) fn set_from_source(
        &mut self,
        ty: &Type,
        spread: l::SpreadKind,
        operand: &Value,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let Type::Set(key_ty) = ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "Set source construction result is not a Set",
            ));
        };
        let key_size = self
            .layout_cached(key_ty)
            .ok_or_else(|| self.invalid(Some(pos.clone()), "Set key has no layout"))?
            .size as u64;
        let key_kind = assoc_key_kind(key_ty, self.module);
        let context = &mut *self.context as *mut Context;
        let set = match spread {
            // SAFETY: verified array handle with the Set's element width.
            l::SpreadKind::Array => unsafe {
                ffi::subscript_rt_set_from_array(
                    context,
                    operand.as_handle()?,
                    key_size,
                    key_kind,
                    0,
                )
            },
            l::SpreadKind::FixedArray => {
                let Value::Blob(bytes) = operand else {
                    return Err(type_error("fixed array", operand));
                };
                let count = (bytes.len() as u64).checked_div(key_size).unwrap_or(0);
                // SAFETY: the fixed blob consists of whole elements.
                unsafe {
                    ffi::subscript_rt_set_from_fixed(
                        context,
                        bytes.as_ptr(),
                        count,
                        key_size,
                        key_kind,
                        0,
                    )
                }
            }
            // SAFETY: runtime association traversal owns order and bound.
            l::SpreadKind::SetValues => unsafe {
                ffi::subscript_rt_set_from_assoc(
                    context,
                    operand.as_handle()?,
                    key_size,
                    key_kind,
                    0,
                )
            },
            // SAFETY: runtime string code-point traversal.
            l::SpreadKind::StringCodePoints => unsafe {
                ffi::subscript_rt_set_from_string(
                    context,
                    operand.as_handle()?,
                    key_size,
                    key_kind,
                    0,
                )
            },
        };
        self.check_runtime(pos)?;
        self.root_handle(set);
        Ok(Value::Handle(set))
    }

    pub(super) fn template(
        &mut self,
        parts: &[l::TemplatePart],
        operands: &[Value],
        instruction: &l::Instruction,
    ) -> Result<*mut u8, InterpretError> {
        let mut bytes = Vec::new();
        for part in parts {
            match part {
                l::TemplatePart::Text(text) => bytes.extend_from_slice(text.as_bytes()),
                l::TemplatePart::Operand { index, format } => {
                    let value = operands.get(*index as usize).ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("template operand {index} is missing"),
                        )
                    })?;
                    bytes.extend_from_slice(&self.format_value(value, *format)?);
                }
            }
        }
        self.alloc_string(&bytes, &instruction.pos)
    }

    fn format_value(
        &self,
        value: &Value,
        format: l::FormatKind,
    ) -> Result<Vec<u8>, InterpretError> {
        Ok(match (format, value) {
            (l::FormatKind::I32, Value::I(value)) => {
                subscript_runtime::fmt::fmt_i32(*value as i32).into_bytes()
            }
            (l::FormatKind::StringAlias(alias), Value::I(value)) => {
                let alias = self
                    .module
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| self.invalid(None, "string alias is missing"))?;
                let index = if let Some(wire) = &alias.wire_values {
                    wire.iter()
                        .position(|candidate| i64::from(*candidate) == *value)
                } else {
                    usize::try_from(*value).ok()
                };
                alias
                    .members
                    .get(index.unwrap_or(usize::MAX))
                    .ok_or_else(|| self.invalid(None, "string alias value has no member"))?
                    .as_bytes()
                    .to_vec()
            }
            (l::FormatKind::I64, Value::I(value)) => {
                subscript_runtime::fmt::fmt_i64(*value).into_bytes()
            }
            (l::FormatKind::U32, Value::U(value)) => {
                subscript_runtime::fmt::fmt_u32(*value as u32).into_bytes()
            }
            (l::FormatKind::F16, Value::U(value)) => {
                subscript_runtime::fmt::fmt_f64(ffi::subscript_rt_f16_to_f64(*value as u16))
                    .into_bytes()
            }
            (l::FormatKind::U64, Value::U(value)) => {
                subscript_runtime::fmt::fmt_u64(*value).into_bytes()
            }
            (l::FormatKind::F32, Value::F32(value)) => {
                subscript_runtime::fmt::fmt_f32(*value).into_bytes()
            }
            (l::FormatKind::F64, Value::F64(value)) => {
                subscript_runtime::fmt::fmt_f64(*value).into_bytes()
            }
            (l::FormatKind::Bool, Value::Bool(value)) => {
                subscript_runtime::fmt::fmt_bool(*value).into_bytes()
            }
            (l::FormatKind::Str, Value::Handle(handle)) => self.string_bytes(*handle)?,
            (_, other) => return Err(type_error("interpolatable scalar", other)),
        })
    }

    pub(super) fn iterator_create(
        &mut self,
        kind: l::ForOfKind,
        bound_kind: l::IteratorBoundKind,
        subject: Value,
        subject_ty: Option<&Type>,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let assoc_bound = if matches!(
            kind,
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
        ) {
            // SAFETY: verified Map/Set subject.
            Some(unsafe {
                ffi::subscript_rt_assoc_iter_begin(&mut *self.context, subject.as_handle()?, 0)
            })
        } else {
            None
        };
        self.check_runtime(pos)?;
        let bound = if let Some(bound) = assoc_bound {
            i64::try_from(bound)
                .map_err(|_| self.invalid(Some(pos.clone()), "iterator bound exceeds i64"))?
        } else {
            match (subject_ty, &subject) {
                (Some(Type::Array(_)), Value::Handle(array)) => {
                    i64::from(unsafe { self.context.array_len(*array) })
                }
                (Some(Type::Str), Value::Handle(string)) => {
                    i64::from(ffi_len_string(&self.context, *string))
                }
                (Some(Type::FixedArray(_, count)), Value::Blob(_)) => i64::from(*count),
                _ => {
                    return Err(self.invalid(
                        Some(pos.clone()),
                        "iterator cursor kind and subject disagree",
                    ));
                }
            }
        };
        let assoc_probe_size = match result_ty {
            Some(l::ValueType::Iterator(iterator))
                if matches!(
                    kind,
                    l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
                ) =>
            {
                Some(
                    self.layout_cached(&iterator.element)
                        .ok_or_else(|| {
                            self.invalid(Some(pos.clone()), "association element has no layout")
                        })?
                        .size,
                )
            }
            _ => None,
        };
        let mut position = if matches!(
            kind,
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse
        ) {
            bound - 1
        } else {
            0
        };
        if matches!(
            kind,
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
        ) {
            position = self.next_live_assoc_position(
                &subject,
                kind,
                assoc_probe_size,
                position,
                bound,
                pos,
            )?;
        }
        Ok(Value::Iterator(Rc::new(IteratorCursor {
            kind,
            bound_kind,
            subject,
            bound,
            position,
            assoc_probe_size,
        })))
    }

    pub(super) fn iterator_has_next(
        &mut self,
        cursor: &Rc<IteratorCursor>,
        index: i64,
        bound: i64,
        pos: &Pos,
    ) -> Result<bool, InterpretError> {
        let current = self.iterator_current_bound(cursor, pos)?;
        let effective = if cursor.bound_kind == l::IteratorBoundKind::Fixed {
            bound.min(current)
        } else {
            current
        };
        let position = if matches!(
            cursor.kind,
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
        ) {
            index
        } else {
            cursor.position
        };
        Ok(position >= 0 && position < effective)
    }

    pub(super) fn iterator_bound(
        &mut self,
        cursor: &Value,
        pos: &Pos,
    ) -> Result<i32, InterpretError> {
        let cursor = cursor.as_iterator()?;
        let bound = if cursor.bound_kind == l::IteratorBoundKind::Fixed {
            cursor.bound
        } else {
            self.iterator_current_bound(cursor, pos)?
        };
        i32::try_from(bound).map_err(|_| self.invalid(None, "iterator bound exceeds i32"))
    }

    fn iterator_current_bound(
        &mut self,
        cursor: &IteratorCursor,
        pos: &Pos,
    ) -> Result<i64, InterpretError> {
        let bound = match cursor.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::ArrayValuesReverse
            | l::ForOfKind::ArrayKeysReverse => {
                i64::from(unsafe { self.context.array_len(cursor.subject.as_handle()?) })
            }
            l::ForOfKind::FixedArrayValues => cursor.bound,
            l::ForOfKind::StringCodePoints => {
                i64::from(ffi_len_string(&self.context, cursor.subject.as_handle()?))
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let bound = unsafe {
                    ffi::subscript_rt_assoc_iter_begin(
                        &mut *self.context,
                        cursor.subject.as_handle()?,
                        0,
                    )
                };
                self.check_runtime(pos)?;
                i64::try_from(bound)
                    .map_err(|_| self.invalid(Some(pos.clone()), "iterator bound exceeds i64"))?
            }
        };
        Ok(bound.max(0))
    }

    pub(super) fn iterator_value(
        &mut self,
        cursor: &Value,
        index: i64,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let cursor = cursor.as_iterator()?;
        let ty = match result_ty {
            Some(l::ValueType::Data(ty)) => ty,
            _ => return Err(self.invalid(Some(pos.clone()), "IteratorValue has no data result")),
        };
        match cursor.kind {
            l::ForOfKind::ArrayKeys | l::ForOfKind::ArrayKeysReverse => {
                Ok(Value::I(if cursor.kind == l::ForOfKind::ArrayKeys {
                    index
                } else {
                    cursor.position
                }))
            }
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayValuesReverse => {
                let array = cursor.subject.as_handle()?;
                let position = if cursor.kind == l::ForOfKind::ArrayValues {
                    index
                } else {
                    cursor.position
                };
                // SAFETY: HasNext established index < captured bound; runtime also
                // checks the current live array length after removals.
                let pointer = unsafe { self.context.array_elem_ptr(array, position as i32, 0) };
                self.check_runtime(pos)?;
                let layout = self.layout_cached(ty).ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "iterator element has no layout")
                })?;
                // SAFETY: runtime returned one element pointer.
                self.unpack(ty, unsafe {
                    std::slice::from_raw_parts(pointer, layout.size)
                })
            }
            l::ForOfKind::FixedArrayValues => {
                let Value::Blob(bytes) = &cursor.subject else {
                    return Err(type_error("fixed array", &cursor.subject));
                };
                let layout = self.layout_cached(ty).ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "iterator element has no layout")
                })?;
                let offset = cursor.position as usize * align_up(layout.size, layout.align);
                self.unpack(ty, bytes.get(offset..).unwrap_or_default())
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let layout = self.layout_cached(ty).ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "association element has no layout")
                })?;
                let mut bytes = vec![0; layout.size];
                // SAFETY: traversal token, index, selection, and output follow the
                // runtime's fixed-bound association protocol.
                let active = unsafe {
                    ffi::subscript_rt_assoc_iter_copy(
                        &mut *self.context,
                        cursor.subject.as_handle()?,
                        cursor.position as u64,
                        u32::from(cursor.kind == l::ForOfKind::MapValues),
                        bytes.as_mut_ptr(),
                        0,
                    )
                };
                self.check_runtime(pos)?;
                if active == 0 {
                    return Err(self.invalid(
                        Some(pos.clone()),
                        "iteration selected an entry removed before its visit",
                    ));
                }
                self.unpack(ty, &bytes)
            }
            l::ForOfKind::StringCodePoints => {
                let mut next = cursor.position as i32;
                // SAFETY: runtime performs UTF-8 scalar stepping and returns its
                // interned one-code-point string.
                let value = unsafe {
                    ffi::subscript_rt_str_iter_code_point(
                        &mut *self.context,
                        cursor.subject.as_handle()?,
                        cursor.position as i32,
                        &mut next,
                        0,
                    )
                };
                self.check_runtime(pos)?;
                self.root_handle(value);
                Ok(Value::Handle(value))
            }
        }
    }

    pub(super) fn iterator_advance(
        &mut self,
        cursor: &Value,
        bound: i64,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let cursor = cursor.as_iterator()?;
        let current_bound = self.iterator_current_bound(cursor, pos)?;
        let effective_bound = if cursor.bound_kind == l::IteratorBoundKind::Fixed {
            bound.min(current_bound)
        } else {
            current_bound
        };
        let position = match cursor.kind {
            l::ForOfKind::StringCodePoints => {
                let bytes = self.string_bytes(cursor.subject.as_handle()?)?;
                let start = usize::try_from(cursor.position)
                    .map_err(|_| self.invalid(Some(pos.clone()), "negative string cursor"))?;
                let rest = bytes.get(start..).ok_or_else(|| {
                    self.invalid(
                        Some(pos.clone()),
                        "string cursor exceeds its captured bound",
                    )
                })?;
                let text = std::str::from_utf8(rest).map_err(|_| {
                    self.invalid(Some(pos.clone()), "string cursor does not name UTF-8")
                })?;
                let width = text.chars().next().map_or(0, char::len_utf8);
                cursor.position.saturating_add(width as i64)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => self
                .next_live_assoc_position(
                    &cursor.subject,
                    cursor.kind,
                    cursor.assoc_probe_size,
                    cursor.position.saturating_add(1),
                    effective_bound,
                    pos,
                )?,
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse => {
                if cursor.position <= 0 || effective_bound <= 0 {
                    -1
                } else {
                    (cursor.position - 1).min(effective_bound - 1)
                }
            }
            _ => cursor.position.saturating_add(1),
        };
        Ok(Value::Iterator(Rc::new(IteratorCursor {
            kind: cursor.kind,
            bound_kind: cursor.bound_kind,
            subject: cursor.subject.clone(),
            bound: cursor.bound,
            position,
            assoc_probe_size: cursor.assoc_probe_size,
        })))
    }

    fn next_live_assoc_position(
        &mut self,
        subject: &Value,
        kind: l::ForOfKind,
        probe_size: Option<usize>,
        mut position: i64,
        bound: i64,
        pos: &Pos,
    ) -> Result<i64, InterpretError> {
        let width = probe_size.ok_or_else(|| {
            self.invalid(Some(pos.clone()), "association cursor has no probe layout")
        })?;
        let mut probe = vec![0_u8; width.max(1)];
        while position < bound {
            let active = unsafe {
                ffi::subscript_rt_assoc_iter_copy(
                    &mut *self.context,
                    subject.as_handle()?,
                    position as u64,
                    u32::from(kind == l::ForOfKind::MapValues),
                    probe.as_mut_ptr(),
                    0,
                )
            };
            self.check_runtime(pos)?;
            if active != 0 {
                break;
            }
            position += 1;
        }
        Ok(position)
    }

    pub(super) fn iter_result(
        &self,
        done: bool,
        value: Value,
        value_ty: &Type,
    ) -> Result<Value, InterpretError> {
        let layout = self
            .layout_cached(value_ty)
            .ok_or_else(|| self.invalid(None, "generator value has no layout"))?;
        let offset = align_up(1, layout.align);
        let total = align_up(offset + layout.size, layout.align);
        let mut bytes = vec![0; total];
        bytes[0] = u8::from(done);
        self.pack_into(value_ty, &value, &mut bytes[offset..])?;
        Ok(Value::Blob(bytes))
    }
}
