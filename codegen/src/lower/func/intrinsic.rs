//! The runtime intrinsics for Date, Array, Map, Set, context bytes, and Workers.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn intrinsic_name(&self, intrinsic: &l::Intrinsic) -> Result<&str, String> {
        self.ml
            .lir
            .intrinsic_operations
            .iter()
            .find(|operation| {
                operation.family == intrinsic.family && operation.operation == intrinsic.operation
            })
            .map(|operation| operation.semantic_name.as_str())
            .ok_or_else(|| {
                internal(format!(
                    "intrinsic {:?}.{} is missing",
                    intrinsic.family, intrinsic.operation
                ))
            })
    }

    pub(super) fn simple_runtime_intrinsic(
        &mut self,
        function: cranelift_module::FuncId,
        operands: &[RV],
        position: Option<&Pos>,
        checked: bool,
        bool_result: bool,
    ) -> Result<RV, String> {
        let mut arguments = vec![self.ctx];
        for value in operands {
            arguments.push(self.expect_scalar(*value)?);
        }
        if let Some(position) = position {
            let position = self.position_id(position);
            arguments.push(self.iconst(types::I32, position));
        }
        let result = self.call_runtime(function, &arguments, checked)?;
        Ok(match result {
            Some(value) if bool_result => {
                RV::Scalar(self.builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
            }
            Some(value) => RV::Scalar(value),
            None => RV::None,
        })
    }

    pub(super) fn intrinsic_call(
        &mut self,
        intrinsic: &l::Intrinsic,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let name = self.intrinsic_name(intrinsic)?.to_string();
        if name != "UnsafeDelete"
            && traps
                .iter()
                .any(|trap| trap.kind == l::TrapKind::DevOnlyLifetime)
        {
            let receiver = self.expect_scalar(
                *operands
                    .first()
                    .ok_or_else(|| internal(format!("{name} has no lifetime operand")))?,
            )?;
            for trap in traps {
                if trap.kind == l::TrapKind::DevOnlyLifetime {
                    self.emit_trap(trap, TrapOperand::Value(receiver))?;
                }
            }
        }
        let checked = traps
            .iter()
            .any(|trap| matches!(trap.kind, l::TrapKind::Allocation | l::TrapKind::Call));
        let result = match intrinsic.family {
            l::IntrinsicFamily::Ambient => match name.as_str() {
                "Print" => {
                    let value = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("Print has no operand"))?,
                    )?;
                    self.call_runtime(self.ml.rt.print, &[self.ctx, value], false)?;
                    RV::None
                }
                "Collect" => {
                    self.call_runtime(self.ml.rt.collect, &[self.ctx], false)?;
                    RV::None
                }
                "UnsafeDelete" => {
                    let value = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("UnsafeDelete has no operand"))?,
                    )?;
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    self.call_runtime(self.ml.rt.delete, &[self.ctx, value, position], false)?;
                    for trap in traps {
                        if trap.kind == l::TrapKind::DevOnlyLifetime {
                            self.emit_trap(trap, TrapOperand::Pending)?;
                        }
                    }
                    RV::None
                }
                "Unreachable" => {
                    let trap = traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::Unreachable)
                        .ok_or_else(|| internal("Unreachable has no trap"))?;
                    self.emit_trap(trap, TrapOperand::Pending)?;
                    RV::None
                }
                other => return Err(internal(format!("unknown Ambient intrinsic {other}"))),
            },
            l::IntrinsicFamily::Math => {
                let function = *self
                    .ml
                    .rt
                    .math
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Math.{name} operation is out of range")))?;
                self.simple_runtime_intrinsic(function, operands, None, checked, false)?
            }
            l::IntrinsicFamily::Number => {
                let function = *self
                    .ml
                    .rt
                    .num
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Number.{name} operation is out of range")))?;
                let bool_result = matches!(
                    name.as_str(),
                    "IsNaN" | "IsFinite" | "IsInteger" | "IsSafeInteger"
                );
                let takes_position = !bool_result;
                self.simple_runtime_intrinsic(
                    function,
                    operands,
                    takes_position.then_some(pos),
                    checked,
                    bool_result,
                )?
            }
            l::IntrinsicFamily::Json => {
                let function = *self
                    .ml
                    .rt
                    .json
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Json.{name} operation is out of range")))?;
                let bool_result = matches!(
                    name.as_str(),
                    "Visit" | "ParseIsKind" | "ParseNumberFits" | "ParseBool"
                );
                self.simple_runtime_intrinsic(function, operands, Some(pos), checked, bool_result)?
            }
            l::IntrinsicFamily::String => {
                let function = *self
                    .ml
                    .rt
                    .str_ops
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("String.{name} operation is out of range")))?;
                let bool_result = matches!(name.as_str(), "Includes" | "StartsWith" | "EndsWith");
                let takes_position = !matches!(
                    name.as_str(),
                    "IndexOf" | "LastIndexOf" | "Includes" | "StartsWith" | "EndsWith"
                );
                self.simple_runtime_intrinsic(
                    function,
                    operands,
                    takes_position.then_some(pos),
                    checked,
                    bool_result,
                )?
            }
            l::IntrinsicFamily::Regex => {
                let function = *self
                    .ml
                    .rt
                    .regex_ops
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Regex.{name} operation is out of range")))?;
                self.simple_runtime_intrinsic(
                    function,
                    operands,
                    Some(pos),
                    checked,
                    name == "Test",
                )?
            }
            l::IntrinsicFamily::Date => self.date_intrinsic(&name, operands, checked, pos)?,
            l::IntrinsicFamily::Array => self.array_intrinsic(
                intrinsic,
                &name,
                operands,
                parameter_types,
                return_type,
                checked,
                pos,
            )?,
            l::IntrinsicFamily::Map => self.map_intrinsic(
                intrinsic,
                &name,
                operands,
                parameter_types,
                return_type,
                checked,
                pos,
            )?,
            l::IntrinsicFamily::Set => self.set_intrinsic(
                intrinsic,
                &name,
                operands,
                parameter_types,
                return_type,
                checked,
                pos,
            )?,
            l::IntrinsicFamily::ContextBytes => {
                self.context_bytes_intrinsic(intrinsic, &name, operands, checked, pos)?
            }
            l::IntrinsicFamily::Worker => {
                self.worker_intrinsic(intrinsic, &name, operands, checked)?
            }
        };
        if checked {
            for trap in traps {
                if matches!(trap.kind, l::TrapKind::Allocation | l::TrapKind::Call) {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
            }
        }
        Ok(result)
    }

    fn date_intrinsic(
        &mut self,
        name: &str,
        operands: &[RV],
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let scalar = |this: &Self, index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Date.{name} operand {index} is missing")))
                .and_then(|value| this.expect_scalar(value))
        };
        match name {
            "New" => {
                let value = scalar(self, 0)?;
                self.simple_runtime_intrinsic(
                    self.ml.rt.date_new,
                    &[RV::Scalar(value)],
                    Some(pos),
                    checked,
                    false,
                )
            }
            "Utc" => self.simple_runtime_intrinsic(
                self.ml.rt.date_utc,
                operands,
                Some(pos),
                checked,
                false,
            ),
            "Now" => {
                self.simple_runtime_intrinsic(self.ml.rt.date_now, operands, None, checked, false)
            }
            "ToIso" => self.simple_runtime_intrinsic(
                self.ml.rt.date_to_iso,
                operands,
                Some(pos),
                checked,
                false,
            ),
            accessor => {
                let code = match accessor {
                    "GetUtcFullYear" => 0,
                    "GetUtcMonth" => 1,
                    "GetUtcDate" => 2,
                    "GetUtcDay" => 3,
                    "GetUtcHours" => 4,
                    "GetUtcMinutes" => 5,
                    "GetUtcSeconds" => 6,
                    "GetUtcMilliseconds" => 7,
                    other => return Err(internal(format!("unknown Date intrinsic {other}"))),
                };
                let value = scalar(self, 0)?;
                let code = self.iconst(types::I32, code);
                let result = self
                    .call_runtime(self.ml.rt.date_get, &[self.ctx, value, code], checked)?
                    .ok_or_else(|| internal("Date accessor has no result"))?;
                Ok(RV::Scalar(result))
            }
        }
    }

    fn array_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let function = *self
            .ml
            .rt
            .arr_ops
            .get(intrinsic.operation as usize)
            .ok_or_else(|| internal(format!("Array.{name} operation is out of range")))?;
        let receiver_ty = parameter_types
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver type")))?;
        let (element, fixed_count) = match receiver_ty {
            l::ValueType::Data(Type::Array(element)) => ((**element).clone(), None),
            l::ValueType::Data(Type::FixedArray(element, count)) => {
                ((**element).clone(), Some(*count))
            }
            other => return Err(internal(format!("Array.{name} receiver is {other:?}"))),
        };
        let receiver = *operands
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver")))?;
        let receiver = if fixed_count.is_some() {
            self.expect_aggregate(receiver)?
        } else {
            self.expect_scalar(receiver)?
        };
        let function = if fixed_count.is_some() {
            self.ml
                .rt
                .fixed_arr_ops
                .get(intrinsic.operation as usize)
                .copied()
                .flatten()
                .ok_or_else(|| internal(format!("Array.{name} is not a FixedArray method")))?
        } else {
            function
        };
        let scalar = |this: &Self, index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Array.{name} operand {index} is missing")))
                .and_then(|value| this.expect_scalar(value))
        };
        let callback = |this: &Self| {
            operands
                .get(1)
                .copied()
                .ok_or_else(|| internal(format!("Array.{name} callback is missing")))
                .and_then(|value| this.expect_pair(value))
        };
        let callback_indexed = || -> Result<bool, String> {
            let expected = match name {
                "ForEach" | "Map" | "Filter" | "Some" | "Every" | "FindIndex" => 2,
                "Reduce" | "ReduceRight" => 3,
                other => return Err(internal(format!("Array.{other} has no indexed callback"))),
            };
            let l::ValueType::Data(Type::Func(function)) = parameter_types
                .get(1)
                .ok_or_else(|| internal(format!("Array.{name} callback type is missing")))?
            else {
                return Err(internal(format!("Array.{name} callback is not a function")));
            };
            match function.params.len() {
                arity if arity + 1 == expected => Ok(false),
                arity if arity == expected => Ok(true),
                arity => Err(internal(format!(
                    "Array.{name} callback arity {arity} escaped the checker"
                ))),
            }
        };
        match name {
            "IndexOf" | "LastIndexOf" | "Includes" => {
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal(format!("Array.{name} search value is missing")))?;
                let value = self.materialize(value, &element)?;
                let kind = array_element_kind(self.ml.lir, &element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                let result = self
                    .call_runtime(function, &[self.ctx, receiver, value, kind], checked)?
                    .ok_or_else(|| internal(format!("Array.{name} has no result")))?;
                Ok(RV::Scalar(if name == "Includes" {
                    self.builder.ins().icmp_imm(IntCC::NotEqual, result, 0)
                } else {
                    result
                }))
            }
            "Join" => {
                let separator = scalar(self, 1)?;
                let kind = array_format_kind(&element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(
                        function,
                        &[self.ctx, receiver, separator, kind, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Array.Join has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Slice" => {
                let start = scalar(self, 1)?;
                let end = scalar(self, 2)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(
                        function,
                        &[self.ctx, receiver, start, end, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Array.Slice has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Fill" => {
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal("Array.Fill value is missing"))?;
                let value = self.materialize(value, &element)?;
                let start = scalar(self, 2)?;
                let end = scalar(self, 3)?;
                self.call_runtime(function, &[self.ctx, receiver, value, start, end], checked)?;
                Ok(RV::Scalar(receiver))
            }
            "Reverse" => {
                self.call_runtime(function, &[self.ctx, receiver], checked)?;
                Ok(RV::Scalar(receiver))
            }
            "Concat" => {
                let other = scalar(self, 1)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(function, &[self.ctx, receiver, other, position], checked)?
                    .ok_or_else(|| internal("Array.Concat has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Splice" => {
                let start = scalar(self, 1)?;
                let delete_count = scalar(self, 2)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(
                        function,
                        &[self.ctx, receiver, start, delete_count, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Array.Splice has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Shift" => {
                let (size, align) = self.ml.layouts.size_align(&element)?;
                let output = self.stack_slot(size.max(8), align.max(8));
                self.zero_bytes(output, size.max(8), align.max(8));
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(function, &[self.ctx, receiver, output, position], checked)?;
                self.load_data(&element, output, 0)
            }
            "Unshift" => {
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal("Array.Unshift value is missing"))?;
                let value = self.materialize(value, &element)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(function, &[self.ctx, receiver, value, position], checked)?
                    .ok_or_else(|| internal("Array.Unshift has no result"))?;
                Ok(RV::Scalar(result))
            }
            "CopyWithin" => {
                let target = scalar(self, 1)?;
                let start = scalar(self, 2)?;
                let end = scalar(self, 3)?;
                self.call_runtime(function, &[self.ctx, receiver, target, start, end], checked)?;
                Ok(RV::Scalar(receiver))
            }
            "ForEach" | "Filter" | "Some" | "Every" | "FindIndex" => {
                let (code, environment) = callback(self)?;
                let indexed = callback_indexed()?;
                let kind = array_element_kind(self.ml.lir, &element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                let indexed = self.iconst(types::I32, i64::from(indexed));
                let mut arguments = vec![self.ctx, receiver];
                if let Some(count) = fixed_count {
                    let stride = self.ml.layouts.stride(&element)?;
                    arguments.push(self.iconst(types::I64, i64::from(count)));
                    arguments.push(self.iconst(types::I64, i64::from(stride)));
                }
                arguments.extend([code, environment, kind]);
                if name == "Filter" {
                    let position = self.position_id(pos);
                    arguments.push(self.iconst(types::I32, position));
                }
                arguments.push(indexed);
                let result = self.call_runtime(function, &arguments, checked)?;
                Ok(match name {
                    "ForEach" => RV::None,
                    "Some" | "Every" => {
                        let result = result
                            .ok_or_else(|| internal(format!("Array.{name} has no result")))?;
                        RV::Scalar(self.builder.ins().icmp_imm(IntCC::NotEqual, result, 0))
                    }
                    _ => RV::Scalar(
                        result.ok_or_else(|| internal(format!("Array.{name} has no result")))?,
                    ),
                })
            }
            "Sort" => {
                let (code, environment) = callback(self)?;
                let kind = array_element_kind(self.ml.lir, &element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                self.call_runtime(
                    function,
                    &[self.ctx, receiver, code, environment, kind],
                    checked,
                )?;
                Ok(RV::Scalar(receiver))
            }
            "Map" => {
                let (code, environment) = callback(self)?;
                let indexed = callback_indexed()?;
                let l::ValueType::Data(Type::Array(result_element)) =
                    return_type.ok_or_else(|| internal("Array.Map result type is missing"))?
                else {
                    return Err(internal("Array.Map result is not an array"));
                };
                let source_kind = array_element_kind(self.ml.lir, &element)?;
                let result_kind = array_element_kind(self.ml.lir, result_element)?;
                let result_stride = self.ml.layouts.stride(result_element)?;
                let mut arguments = vec![self.ctx, receiver];
                if let Some(count) = fixed_count {
                    let stride = self.ml.layouts.stride(&element)?;
                    arguments.push(self.iconst(types::I64, i64::from(count)));
                    arguments.push(self.iconst(types::I64, i64::from(stride)));
                }
                arguments.extend([
                    code,
                    environment,
                    self.iconst(types::I32, i64::from(source_kind)),
                    self.iconst(types::I32, i64::from(result_kind)),
                    self.iconst(types::I64, i64::from(result_stride)),
                ]);
                let position = self.position_id(pos);
                arguments.push(self.iconst(types::I32, position));
                arguments.push(self.iconst(types::I32, i64::from(indexed)));
                let result = self
                    .call_runtime(function, &arguments, checked)?
                    .ok_or_else(|| internal("Array.Map has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Reduce" | "ReduceRight" => {
                let (code, environment) = callback(self)?;
                let indexed = callback_indexed()?;
                let accumulator_ty = data_type(
                    return_type.ok_or_else(|| internal("Array.Reduce result type is missing"))?,
                )?;
                let initial = *operands
                    .get(2)
                    .ok_or_else(|| internal("Array.Reduce initial value is missing"))?;
                let accumulator = self.materialize(initial, accumulator_ty)?;
                let element_kind = array_element_kind(self.ml.lir, &element)?;
                let accumulator_kind = array_element_kind(self.ml.lir, accumulator_ty)?;
                let accumulator_stride = self.ml.layouts.stride(accumulator_ty)?;
                let mut arguments = vec![self.ctx, receiver];
                if let Some(count) = fixed_count {
                    let stride = self.ml.layouts.stride(&element)?;
                    arguments.push(self.iconst(types::I64, i64::from(count)));
                    arguments.push(self.iconst(types::I64, i64::from(stride)));
                }
                arguments.extend([
                    code,
                    environment,
                    self.iconst(types::I32, i64::from(element_kind)),
                    self.iconst(types::I32, i64::from(accumulator_kind)),
                    self.iconst(types::I64, i64::from(accumulator_stride)),
                    accumulator,
                    self.iconst(types::I32, i64::from(indexed)),
                ]);
                self.call_runtime(function, &arguments, checked)?;
                self.load_data(accumulator_ty, accumulator, 0)
            }
            other => Err(internal(format!(
                "Array.{other} needs its typed runtime adapter"
            ))),
        }
    }

    fn map_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let function = *self
            .ml
            .rt
            .map_ops
            .get(intrinsic.operation as usize)
            .ok_or_else(|| internal(format!("Map.{name} operation is out of range")))?;
        if name == "GroupBy" {
            let (key, element) = match (return_type, parameter_types.first()) {
                (
                    Some(l::ValueType::Data(Type::Map(key, value))),
                    Some(l::ValueType::Data(Type::Array(element))),
                ) => match &**value {
                    Type::Array(group_element) if **group_element == **element => {
                        ((**key).clone(), (**element).clone())
                    }
                    other => {
                        return Err(internal(format!("Map.GroupBy result value is {other:?}")))
                    }
                },
                other => return Err(internal(format!("Map.GroupBy shape is {other:?}"))),
            };
            let items = self.expect_scalar(
                *operands
                    .first()
                    .ok_or_else(|| internal("Map.GroupBy items are missing"))?,
            )?;
            self.live_check(items, pos)?;
            let (code, environment) = self.expect_pair(
                *operands
                    .get(1)
                    .ok_or_else(|| internal("Map.GroupBy callback is missing"))?,
            )?;
            let bridge = define_group_bridge(self.ml, &element, &key)?;
            let bridge = self
                .ml
                .module
                .declare_func_in_func(bridge, self.builder.func);
            let bridge = self.builder.ins().func_addr(types::I64, bridge);
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let kind = association_key_kind(self.ml.lir, &key)?;
            let position = self.position_id(pos);
            let arguments = [
                self.ctx,
                items,
                code,
                environment,
                bridge,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I32, i64::from(kind)),
                self.iconst(types::I32, position),
            ];
            return self
                .call_runtime(function, &arguments, checked)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Map.GroupBy has no result"));
        }

        let (key, value) = if name == "New" {
            match return_type {
                Some(l::ValueType::Data(Type::Map(key, value))) => {
                    ((**key).clone(), (**value).clone())
                }
                other => return Err(internal(format!("Map.New result is {other:?}"))),
            }
        } else {
            match parameter_types.first() {
                Some(l::ValueType::Data(Type::Map(key, value))) => {
                    ((**key).clone(), (**value).clone())
                }
                other => return Err(internal(format!("Map.{name} receiver is {other:?}"))),
            }
        };
        if name == "New" {
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let (value_size, _) = self.ml.layouts.size_align(&value)?;
            let kind = association_key_kind(self.ml.lir, &key)?;
            let position = self.position_id(pos);
            let arguments = [
                self.ctx,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I64, i64::from(value_size)),
                self.iconst(types::I32, i64::from(kind)),
                self.iconst(types::I32, position),
            ];
            return self
                .call_runtime(function, &arguments, checked)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Map.New has no result"));
        }

        let handle = self.expect_scalar(
            *operands
                .first()
                .ok_or_else(|| internal(format!("Map.{name} receiver is missing")))?,
        )?;
        self.live_check(handle, pos)?;
        let operand = |index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Map.{name} operand {index} is missing")))
        };
        match name {
            "Size" => self
                .call_runtime(function, &[self.ctx, handle], false)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Map.Size has no result")),
            "Get" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let (size, align) = self.ml.layouts.size_align(&value)?;
                let output = self.stack_slot(size.max(8), align.max(8));
                self.zero_bytes(output, size.max(8), align.max(8));
                self.call_runtime(function, &[self.ctx, handle, key_address, output], false)?;
                self.load_data(&value, output, 0)
            }
            "GetOr" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let fallback = self.materialize(operand(2)?, &value)?;
                let (size, align) = self.ml.layouts.size_align(&value)?;
                let slot_size = size.max(8);
                let access_align = 1u32 << slot_size.trailing_zeros();
                let output = self.stack_slot(slot_size, align.max(8));
                self.zero_bytes(output, slot_size, align.max(8).min(access_align));
                self.call_runtime(
                    function,
                    &[self.ctx, handle, key_address, fallback, output],
                    false,
                )?;
                self.load_data(&value, output, 0)
            }
            "Set" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let value_address = self.materialize(operand(2)?, &value)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, key_address, value_address, position],
                    checked,
                )?;
                Ok(RV::Scalar(handle))
            }
            "Has" | "Delete" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let result = self
                    .call_runtime(function, &[self.ctx, handle, key_address], checked)?
                    .ok_or_else(|| internal(format!("Map.{name} has no result")))?;
                Ok(RV::Scalar(self.builder.ins().icmp_imm(
                    IntCC::NotEqual,
                    result,
                    0,
                )))
            }
            "Clear" => {
                self.call_runtime(function, &[self.ctx, handle], false)?;
                Ok(RV::None)
            }
            "ForEach" => {
                let (code, environment) = self.expect_pair(operand(1)?)?;
                let bridge = define_assoc_bridge(self.ml, &key, Some(&value))?;
                let bridge = self
                    .ml
                    .module
                    .declare_func_in_func(bridge, self.builder.func);
                let bridge = self.builder.ins().func_addr(types::I64, bridge);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, code, environment, bridge],
                    checked,
                )?;
                Ok(RV::None)
            }
            other => Err(internal(format!("unknown Map intrinsic {other}"))),
        }
    }

    fn set_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let function = *self
            .ml
            .rt
            .set_ops
            .get(intrinsic.operation as usize)
            .ok_or_else(|| internal(format!("Set.{name} operation is out of range")))?;
        let key = if name == "New" {
            match return_type {
                Some(l::ValueType::Data(Type::Set(key))) => (**key).clone(),
                other => return Err(internal(format!("Set.New result is {other:?}"))),
            }
        } else {
            match parameter_types.first() {
                Some(l::ValueType::Data(Type::Set(key))) => (**key).clone(),
                other => return Err(internal(format!("Set.{name} receiver is {other:?}"))),
            }
        };
        if name == "New" {
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let kind = association_key_kind(self.ml.lir, &key)?;
            let position = self.position_id(pos);
            let arguments = [
                self.ctx,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I32, i64::from(kind)),
                self.iconst(types::I32, position),
            ];
            return self
                .call_runtime(function, &arguments, checked)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Set.New has no result"));
        }

        let handle = self.expect_scalar(
            *operands
                .first()
                .ok_or_else(|| internal(format!("Set.{name} receiver is missing")))?,
        )?;
        self.live_check(handle, pos)?;
        let operand = |index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Set.{name} operand {index} is missing")))
        };
        match name {
            "Size" => self
                .call_runtime(function, &[self.ctx, handle], false)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Set.Size has no result")),
            "Add" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, key_address, position],
                    checked,
                )?;
                Ok(RV::Scalar(handle))
            }
            "Has" | "Delete" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let result = self
                    .call_runtime(function, &[self.ctx, handle, key_address], checked)?
                    .ok_or_else(|| internal(format!("Set.{name} has no result")))?;
                Ok(RV::Scalar(self.builder.ins().icmp_imm(
                    IntCC::NotEqual,
                    result,
                    0,
                )))
            }
            "Clear" => {
                self.call_runtime(function, &[self.ctx, handle], false)?;
                Ok(RV::None)
            }
            "ForEach" => {
                let (code, environment) = self.expect_pair(operand(1)?)?;
                let bridge = define_assoc_bridge(self.ml, &key, None)?;
                let bridge = self
                    .ml
                    .module
                    .declare_func_in_func(bridge, self.builder.func);
                let bridge = self.builder.ins().func_addr(types::I64, bridge);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, code, environment, bridge],
                    checked,
                )?;
                Ok(RV::None)
            }
            "Union" | "Intersection" | "Difference" | "SymmetricDifference" => {
                let other = self.expect_scalar(operand(1)?)?;
                self.live_check(other, pos)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(function, &[self.ctx, handle, other, position], checked)?
                    .map(RV::Scalar)
                    .ok_or_else(|| internal(format!("Set.{name} has no result")))
            }
            "IsSubsetOf" | "IsSupersetOf" | "IsDisjointFrom" => {
                let other = self.expect_scalar(operand(1)?)?;
                self.live_check(other, pos)?;
                let result = self
                    .call_runtime(function, &[self.ctx, handle, other], false)?
                    .ok_or_else(|| internal(format!("Set.{name} has no result")))?;
                Ok(RV::Scalar(self.builder.ins().icmp_imm(
                    IntCC::NotEqual,
                    result,
                    0,
                )))
            }
            other => Err(internal(format!("unknown Set intrinsic {other}"))),
        }
    }

    fn context_bytes_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let ty = intrinsic
            .type_argument
            .as_ref()
            .ok_or_else(|| internal(format!("Context.{name} has no type argument")))?;
        let (size, align) = self.ml.layouts.size_align(ty)?;
        let size_value = self.iconst(types::I32, i64::from(size));
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        match name {
            "BytesOf" => {
                let source = self.expect_aggregate(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Context.BytesOf value is missing"))?,
                )?;
                let handle = self
                    .call_runtime(
                        self.ml.rt.array_from_bytes,
                        &[self.ctx, source, size_value, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Context.BytesOf has no result"))?;
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("Context.BytesOf has no array data"))?;
                for range in self.ml.layouts.padding_ranges(ty)? {
                    let start = self.address_offset(data, i64::from(range.start));
                    self.zero_bytes(start, range.end - range.start, 1);
                }
                Ok(RV::Scalar(handle))
            }
            "BytesInto" => {
                let source = self.expect_aggregate(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Context.BytesInto value is missing"))?,
                )?;
                let target = self.expect_scalar(
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("Context.BytesInto target is missing"))?,
                )?;
                let offset = self.expect_scalar(
                    *operands
                        .get(2)
                        .ok_or_else(|| internal("Context.BytesInto offset is missing"))?,
                )?;
                let range = self
                    .call_runtime(
                        self.ml.rt.array_byte_range,
                        &[self.ctx, target, offset, size_value, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Context.BytesInto has no target range"))?;
                self.copy_bytes(range, source, size, 1);
                for padding in self.ml.layouts.padding_ranges(ty)? {
                    let start = self.address_offset(range, i64::from(padding.start));
                    self.zero_bytes(start, padding.end - padding.start, 1);
                }
                Ok(RV::None)
            }
            "FromBytes" => {
                let bytes = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Context.FromBytes source is missing"))?,
                )?;
                let offset = self.expect_scalar(
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("Context.FromBytes offset is missing"))?,
                )?;
                let range = self
                    .call_runtime(
                        self.ml.rt.array_byte_range,
                        &[self.ctx, bytes, offset, size_value, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Context.FromBytes has no source range"))?;
                let output = self.stack_slot(size, align);
                self.copy_bytes(output, range, size, 1);
                Ok(RV::Aggregate(output))
            }
            other => Err(internal(format!("unknown Context byte intrinsic {other}"))),
        }
    }

    fn worker_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        checked: bool,
    ) -> Result<RV, String> {
        if name == "Spawn" {
            if !operands.is_empty() {
                return Err(internal("Worker.Spawn retained source operands"));
            }
            let index = intrinsic
                .worker_entry
                .ok_or_else(|| internal("Worker.Spawn has no worker entry"))?
                as usize;
            let entry = self
                .ml
                .lir
                .worker_entries
                .get(index)
                .ok_or_else(|| internal(format!("worker entry {index} is missing")))?;
            let input_class = entry.input;
            let output_class = entry.output;
            let initialize = self.ml.func_id(&FnKey::WorkerInit)?;
            let initialize = self
                .ml
                .module
                .declare_func_in_func(initialize, self.builder.func);
            let initialize = self.builder.ins().func_addr(types::I64, initialize);
            let worker = self.ml.func_id(&FnKey::WorkerEntry(index))?;
            let worker = self
                .ml
                .module
                .declare_func_in_func(worker, self.builder.func);
            let worker = self.builder.ins().func_addr(types::I64, worker);
            let input_descriptor = self.ml.worker_message_descriptor_data(input_class)?;
            let input_descriptor = self
                .ml
                .module
                .declare_data_in_func(input_descriptor, self.builder.func);
            let input_descriptor = self
                .builder
                .ins()
                .symbol_value(types::I64, input_descriptor);
            let output_descriptor = self.ml.worker_message_descriptor_data(output_class)?;
            let output_descriptor = self
                .ml
                .module
                .declare_data_in_func(output_descriptor, self.builder.func);
            let output_descriptor = self
                .builder
                .ins()
                .symbol_value(types::I64, output_descriptor);
            return self
                .call_runtime(
                    self.ml.rt.worker_spawn,
                    &[
                        self.ctx,
                        initialize,
                        worker,
                        input_descriptor,
                        output_descriptor,
                    ],
                    checked,
                )?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Worker.Spawn has no result"));
        }

        let expected = match name {
            "Post" | "OutboxPost" => 2,
            "Poll" | "Close" | "Join" | "InboxWait" | "InboxPoll" => 1,
            other => return Err(internal(format!("unknown Worker intrinsic {other}"))),
        };
        if operands.len() != expected {
            return Err(internal(format!(
                "Worker.{name} has {} operand(s), expected {expected}",
                operands.len()
            )));
        }
        let mut arguments = Vec::with_capacity(expected + 1);
        arguments.push(self.ctx);
        for operand in operands {
            arguments.push(self.expect_scalar(*operand)?);
        }
        let function = match name {
            "Post" => self.ml.rt.worker_post,
            "Poll" => self.ml.rt.worker_poll,
            "Close" => self.ml.rt.worker_close,
            "Join" => self.ml.rt.worker_join,
            "InboxWait" => self.ml.rt.worker_inbox_wait,
            "InboxPoll" => self.ml.rt.worker_inbox_poll,
            "OutboxPost" => self.ml.rt.worker_outbox_post,
            _ => unreachable!("validated above"),
        };
        let result = self.call_runtime(function, &arguments, checked)?;
        Ok(match name {
            "Poll" | "InboxWait" | "InboxPoll" => {
                RV::Scalar(result.ok_or_else(|| internal(format!("Worker.{name} has no result")))?)
            }
            _ => RV::None,
        })
    }
}
