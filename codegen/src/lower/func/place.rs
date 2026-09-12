//! Global, field, and element addresses.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn global_address(&mut self, id: l::GlobalId) -> Result<(Value, Type), String> {
        let definition = self
            .ml
            .lir
            .globals
            .get(id.0 as usize)
            .filter(|global| global.id == id)
            .ok_or_else(|| internal(format!("global {} is missing", id.0)))?;
        let (slot, ty) = self
            .ml
            .globals
            .get(&definition.source_name)
            .cloned()
            .ok_or_else(|| internal(format!("global {} has no target slot", id.0)))?;
        let address = match slot {
            GlobalSlot::Data(data) => {
                let global = self.ml.module.declare_data_in_func(data, self.builder.func);
                self.builder.ins().symbol_value(types::I64, global)
            }
            GlobalSlot::Offset(offset) => {
                let base_offset = ctx_off(rtc::Context::globals_offset())?;
                let base = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), self.ctx, base_offset);
                self.address_offset(base, i64::from(offset))
            }
        };
        Ok((address, ty))
    }

    fn field_definition(&self, id: l::FieldId) -> Result<(ClassId, usize, &l::Field), String> {
        self.ml
            .lir
            .classes
            .iter()
            .find_map(|class| {
                class
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.id == id)
                    .map(|(index, field)| (class.id, index, field))
            })
            .ok_or_else(|| internal(format!("field {} is missing", id.0)))
    }

    pub(super) fn field_address(
        &mut self,
        field: l::FieldRef,
        base: RV,
        base_type: &l::ValueType,
        traps: &[l::Trap],
    ) -> Result<(Value, Type), String> {
        match field {
            l::FieldRef::Class(field) => {
                let (class, index, definition) = self.field_definition(field)?;
                let ty = definition.ty.clone();
                let offset = *self
                    .ml
                    .layouts
                    .class(class.0)?
                    .field_offsets
                    .get(index)
                    .ok_or_else(|| internal(format!("field {} has no layout offset", field.0)))?;
                let address = match base_type {
                    l::ValueType::Data(Type::Class(class)) => {
                        if self.ml.layouts.class(class.0)?.is_value {
                            let pointer = self.expect_aggregate(base)?;
                            self.address_offset(pointer, i64::from(offset))
                        } else {
                            let pointer = self.expect_scalar(base)?;
                            for trap in traps {
                                if trap.kind == l::TrapKind::DevOnlyLifetime {
                                    self.emit_trap(trap, TrapOperand::Value(pointer))?;
                                }
                            }
                            self.address_offset(pointer, i64::from(offset))
                        }
                    }
                    l::ValueType::Data(Type::Nullable(inner)) if matches!(inner.as_ref(), Type::Class(id) if *id == class) =>
                    {
                        let pointer = self.expect_scalar(base)?;
                        self.address_offset(pointer, i64::from(offset))
                    }
                    l::ValueType::Address(_) => {
                        let pointer = self.expect_scalar(base)?;
                        self.address_offset(pointer, i64::from(offset))
                    }
                    other => {
                        return Err(internal(format!(
                            "field {} has invalid base {other:?}",
                            field.0
                        )))
                    }
                };
                Ok((address, ty))
            }
            l::FieldRef::IterDone => {
                let address = self.expect_aggregate(base)?;
                Ok((address, Type::Bool))
            }
            l::FieldRef::IterValue => {
                let l::ValueType::Data(Type::IterResult(value)) = base_type else {
                    return Err(internal("IterResult.value has invalid base type"));
                };
                let address = self.expect_aggregate(base)?;
                let offset = self.ml.layouts.iter_result_value_offset(value)?;
                Ok((
                    self.address_offset(address, i64::from(offset)),
                    (**value).clone(),
                ))
            }
        }
    }

    /// Guards the semantic `JsonResult<T>.value` field load with its
    /// materialized sibling `ok` field. Ordinary field loads can carry other
    /// trap kinds, so the LIR trap distinguishes this checked access after
    /// the HIR expression kind has been transcribed away.
    pub(super) fn guard_json_result_value(
        &mut self,
        field: l::FieldRef,
        base: RV,
        traps: &[l::Trap],
    ) -> Result<(), String> {
        let mut json_traps = traps.iter().filter_map(|trap| match trap.kind {
            l::TrapKind::JsonResultValue(ok_field) => Some((trap, ok_field)),
            _ => None,
        });
        let Some((first_trap, ok_field)) = json_traps.next() else {
            return Ok(());
        };
        let l::FieldRef::Class(field) = field else {
            return Err(internal(
                "JSON result trap is attached to a synthetic field",
            ));
        };
        let (class, _, _) = self.field_definition(field)?;
        let definition = self
            .ml
            .lir
            .classes
            .get(class.0)
            .filter(|definition| definition.id == class)
            .ok_or_else(|| internal("JSON result class is missing"))?;
        let ok_index = definition
            .fields
            .iter()
            .position(|field| field.id == ok_field && field.ty == Type::Bool)
            .ok_or_else(|| internal("JSON result guard field id is invalid"))?;
        if json_traps.any(|(_, candidate)| candidate != ok_field) {
            return Err(internal("JSON result traps disagree on the guard field id"));
        }
        if definition.is_value {
            return Err(internal("JSON result unexpectedly has value-class layout"));
        }
        let ok_offset = *self
            .ml
            .layouts
            .class(class.0)?
            .field_offsets
            .get(ok_index)
            .ok_or_else(|| internal("JSON result ok field offset is missing"))?;
        let pointer = self.expect_scalar(base)?;
        let ok = self.load_data(&Type::Bool, pointer, ok_offset as i32)?;
        let ok = self.expect_scalar(ok)?;
        self.emit_trap(first_trap, TrapOperand::Condition(ok))?;
        Ok(())
    }

    pub(super) fn index_address(
        &mut self,
        base: RV,
        base_type: &l::ValueType,
        index: Value,
        index_type: &Type,
        checked: bool,
        traps: &[l::Trap],
    ) -> Result<(Value, Type), String> {
        let (base_address, length, runtime_stride, element) = match base_type {
            l::ValueType::Data(Type::Array(element)) => {
                let handle = self.expect_scalar(base)?;
                for trap in traps {
                    if trap.kind == l::TrapKind::DevOnlyLifetime {
                        self.emit_trap(trap, TrapOperand::Value(handle))?;
                    }
                }
                let length = checked.then(|| {
                    self.builder
                        .ins()
                        .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET)
                });
                let stride =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), handle, ARRAY_ELEM_SIZE_OFFSET);
                let data = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_DATA_OFFSET);
                (data, length, Some(stride), (**element).clone())
            }
            l::ValueType::Data(Type::FixedArray(element, count)) => {
                let address = self.expect_aggregate(base)?;
                let length = checked.then(|| self.iconst(types::I64, i64::from(*count)));
                (address, length, None, (**element).clone())
            }
            l::ValueType::Address(address) => match &address.pointee {
                Type::FixedArray(element, count) => {
                    let base = self.expect_scalar(base)?;
                    let length = checked.then(|| self.iconst(types::I64, i64::from(*count)));
                    (base, length, None, (**element).clone())
                }
                other => {
                    return Err(internal(format!(
                        "indexed address points to invalid type {other:?}"
                    )))
                }
            },
            other => return Err(internal(format!("invalid indexed base {other:?}"))),
        };
        let index64 = if self.builder.func.dfg.value_type(index) == types::I64 {
            index
        } else if is_unsigned(index_type) {
            self.builder.ins().uextend(types::I64, index)
        } else {
            self.builder.ins().sextend(types::I64, index)
        };
        if checked {
            let length = length.ok_or_else(|| internal("checked index has no captured length"))?;
            let index32 = if self.builder.func.dfg.value_type(index) == types::I32 {
                index
            } else {
                self.builder.ins().ireduce(types::I32, index)
            };
            let below = self
                .builder
                .ins()
                .icmp(IntCC::UnsignedLessThan, index64, length);
            let valid = if is_unsigned(index_type) {
                below
            } else {
                let nonnegative =
                    self.builder
                        .ins()
                        .icmp_imm(IntCC::SignedGreaterThanOrEqual, index32, 0);
                self.builder.ins().band(nonnegative, below)
            };
            let trap_length = self.builder.ins().ireduce(types::I32, length);
            for trap in traps {
                if matches!(trap.kind, l::TrapKind::IndexRead | l::TrapKind::IndexWrite) {
                    self.emit_trap(
                        trap,
                        TrapOperand::Index {
                            condition: valid,
                            index: index32,
                            length: trap_length,
                        },
                    )?;
                }
            }
        }
        let offset = if let Some(stride) = runtime_stride {
            self.builder.ins().imul(index64, stride)
        } else {
            let stride = self.ml.layouts.stride(&element)?;
            self.builder.ins().imul_imm(index64, i64::from(stride))
        };
        Ok((self.builder.ins().iadd(base_address, offset), element))
    }
}
