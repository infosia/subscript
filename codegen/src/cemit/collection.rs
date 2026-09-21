//! Array, map, and set intrinsics.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_array_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let receiver_type = operand_types
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver type")))?;
        let (element, fixed_count) = match receiver_type {
            l::ValueType::Data(Type::Array(element)) => (element.as_ref(), None),
            l::ValueType::Data(Type::FixedArray(element, count)) => {
                (element.as_ref(), Some(*count))
            }
            other => return Err(internal(format!("Array.{name} receiver is {other:?}"))),
        };
        let receiver = operands
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver")))?;
        let receiver = if fixed_count.is_some() {
            format!("(const void*)({receiver}).a")
        } else {
            receiver.clone()
        };
        let symbol = array_symbol(name, fixed_count.is_some())?;
        let result_type = target.return_type.as_ref().map(data_type).transpose()?;
        let return_ctype = target
            .return_type
            .as_ref()
            .map(|ty| self.emitter.value_ctype(ty))
            .transpose()?
            .unwrap_or_else(|| "void".into());
        let argument = |index: usize| {
            operands
                .get(index)
                .cloned()
                .ok_or_else(|| internal(format!("Array.{name} operand {index} is missing")))
        };
        let indexed = || -> Result<u32, String> {
            let expected = match name {
                "ForEach" | "Map" | "Filter" | "Some" | "Every" | "FindIndex" => 2,
                "Reduce" | "ReduceRight" => 3,
                other => return Err(internal(format!("Array.{other} has no indexed callback"))),
            };
            let Some(l::ValueType::Data(Type::Func(function))) = operand_types.get(1) else {
                return Err(internal(format!("Array.{name} callback type is missing")));
            };
            match function.params.len() {
                arity if arity + 1 == expected => Ok(0),
                arity if arity == expected => Ok(1),
                arity => Err(internal(format!(
                    "Array.{name} callback arity {arity} escaped the checker"
                ))),
            }
        };
        let call = match name {
            "IndexOf" | "LastIndexOf" | "Includes" => {
                let pointer = self.materialize(out, &argument(1)?, element)?;
                self.emitter.runtime_call(
                    &return_ctype,
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver,
                        pointer,
                        format!("{}u", array_element_kind(self.emitter.module, element)?),
                    ],
                )
            }
            "Join" => {
                let position = self.emitter.pos_id(&instruction.pos);
                self.emitter.runtime_call(
                    &return_ctype,
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver,
                        argument(1)?,
                        format!("{}u", array_format_kind(element)?),
                        format!("{position}u"),
                    ],
                )
            }
            "Slice" | "Concat" | "Splice" | "Unshift" => {
                let mut types = vec!["void*".into(), "void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                match name {
                    "Slice" | "Splice" => {
                        types.extend(["int32_t".into(), "int32_t".into()]);
                        args.extend([argument(1)?, argument(2)?]);
                    }
                    "Concat" => {
                        types.push("void*".into());
                        args.push(argument(1)?);
                    }
                    "Unshift" => {
                        types.push("const void*".into());
                        args.push(self.materialize(out, &argument(1)?, element)?);
                    }
                    _ => unreachable!(),
                }
                types.push("uint32_t".into());
                args.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
                self.emitter
                    .runtime_call(&return_ctype, symbol, &types, &args)
            }
            "Fill" => {
                let pointer = self.materialize(out, &argument(1)?, element)?;
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "int32_t".into(),
                        "int32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        pointer,
                        argument(2)?,
                        argument(3)?,
                    ],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true, true);
            }
            "Reverse" => {
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true, true);
            }
            "Shift" => {
                let destination = result.ok_or_else(|| internal("Array.Shift has no result"))?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver,
                        format!("&{destination}"),
                        format!("{position}u"),
                    ],
                );
                let _ = writeln!(out, "    {call};");
                return self.consume_runtime_traps(out, &instruction.traps, true, true);
            }
            "CopyWithin" => {
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "int32_t".into(),
                        "int32_t".into(),
                        "int32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        argument(1)?,
                        argument(2)?,
                        argument(3)?,
                    ],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true, true);
            }
            "ForEach" | "Filter" | "Some" | "Every" | "FindIndex" => {
                let callback = argument(1)?;
                let mut types = vec!["void*".into(), "const void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                if let Some(count) = fixed_count {
                    types.extend(["uint64_t".into(), "uint64_t".into()]);
                    args.extend([
                        format!("{count}ull"),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(element)?),
                    ]);
                }
                types.extend([
                    "const void*".into(),
                    "const void*".into(),
                    "uint32_t".into(),
                ]);
                args.extend([
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("{}u", array_element_kind(self.emitter.module, element)?),
                ]);
                if name == "Filter" {
                    types.push("uint32_t".into());
                    args.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
                }
                types.push("uint32_t".into());
                args.push(format!("{}u", indexed()?));
                self.emitter
                    .runtime_call(&return_ctype, symbol, &types, &args)
            }
            "Sort" => {
                let callback = argument(1)?;
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        format!("{callback}.code"),
                        format!("{callback}.env"),
                        format!("{}u", array_element_kind(self.emitter.module, element)?),
                    ],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true, true);
            }
            "Map" => {
                let callback = argument(1)?;
                let Type::Array(result_element) =
                    result_type.ok_or_else(|| internal("Array.Map result type is missing"))?
                else {
                    return Err(internal("Array.Map result is not an array"));
                };
                let mut types = vec!["void*".into(), "const void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                if let Some(count) = fixed_count {
                    types.extend(["uint64_t".into(), "uint64_t".into()]);
                    args.extend([
                        format!("{count}ull"),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(element)?),
                    ]);
                }
                types.extend([
                    "const void*".into(),
                    "const void*".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ]);
                args.extend([
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("{}u", array_element_kind(self.emitter.module, element)?),
                    format!(
                        "{}u",
                        array_element_kind(self.emitter.module, result_element)?
                    ),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(result_element)?),
                    format!("{}u", self.emitter.pos_id(&instruction.pos)),
                    format!("{}u", indexed()?),
                ]);
                self.emitter
                    .runtime_call(&return_ctype, symbol, &types, &args)
            }
            "Reduce" | "ReduceRight" => {
                let callback = argument(1)?;
                let accumulator =
                    result_type.ok_or_else(|| internal("Array.Reduce result type is missing"))?;
                let temporary = self.fresh();
                let _ = writeln!(
                    out,
                    "    {} {temporary} = {};",
                    self.emitter.ctype(accumulator)?,
                    argument(2)?
                );
                let mut types = vec!["void*".into(), "const void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                if let Some(count) = fixed_count {
                    types.extend(["uint64_t".into(), "uint64_t".into()]);
                    args.extend([
                        format!("{count}ull"),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(element)?),
                    ]);
                }
                types.extend([
                    "const void*".into(),
                    "const void*".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                    "uint64_t".into(),
                    "void*".into(),
                    "uint32_t".into(),
                ]);
                args.extend([
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("{}u", array_element_kind(self.emitter.module, element)?),
                    format!("{}u", array_element_kind(self.emitter.module, accumulator)?),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(accumulator)?),
                    format!("&{temporary}"),
                    format!("{}u", indexed()?),
                ]);
                let call = self.emitter.runtime_call("void", symbol, &types, &args);
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &temporary)?;
                return self.consume_runtime_traps(out, &instruction.traps, true, true);
            }
            other => return Err(internal(format!("unknown Array intrinsic {other}"))),
        };
        if let Some(result) = result {
            let _ = writeln!(out, "    {result} = {call};");
        } else {
            let _ = writeln!(out, "    {call};");
        }
        self.consume_runtime_traps(out, &instruction.traps, true, true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_map_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if name == "GroupBy" {
            let Some(l::ValueType::Data(Type::Array(element))) = operand_types.first() else {
                return Err(internal("Map.GroupBy items type is not an array"));
            };
            let Some(l::ValueType::Data(Type::Map(key, value))) = target.return_type.as_ref()
            else {
                return Err(internal("Map.GroupBy result is not a map"));
            };
            if !matches!(value.as_ref(), Type::Array(group_element) if group_element == element) {
                return Err(internal(
                    "Map.GroupBy result value is not the source element array",
                ));
            }
            let callback = operands
                .get(1)
                .ok_or_else(|| internal("Map.GroupBy callback is missing"))?;
            let bridge = self.emitter.define_group_bridge(element, key)?;
            let position = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_map_group_by",
                &[
                    "void*".into(),
                    "void*".into(),
                    "const void*".into(),
                    "const void*".into(),
                    "const void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    operands[0].clone(),
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("(const void*)&{bridge}"),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(key)?),
                    format!("{}u", association_key_kind(self.emitter.module, key)?),
                    format!("{position}u"),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true, true);
        }
        let (key, value) = if name == "New" {
            let Some(l::ValueType::Data(Type::Map(key, value))) = target.return_type.as_ref()
            else {
                return Err(internal("Map.New result is not a map"));
            };
            (key.as_ref(), value.as_ref())
        } else {
            let Some(l::ValueType::Data(Type::Map(key, value))) = operand_types.first() else {
                return Err(internal(format!("Map.{name} receiver is not a map")));
            };
            (key.as_ref(), value.as_ref())
        };
        if name == "New" {
            let position = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_map_new",
                &[
                    "void*".into(),
                    "uint64_t".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(key)?),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(value)?),
                    format!("{}u", association_key_kind(self.emitter.module, key)?),
                    format!("{position}u"),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true, true);
        }
        let receiver = operands
            .first()
            .ok_or_else(|| internal(format!("Map.{name} receiver is missing")))?;
        let argument = |index: usize| {
            operands
                .get(index)
                .cloned()
                .ok_or_else(|| internal(format!("Map.{name} operand {index} is missing")))
        };
        match name {
            "Size" => {
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_assoc_size",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                self.assign(out, result, &call)?;
            }
            "Get" | "GetOr" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let destination =
                    result.ok_or_else(|| internal(format!("Map.{name} has no result")))?;
                if name == "Get" {
                    let call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_map_get",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "void*".into(),
                        ],
                        &[
                            "ctx".into(),
                            receiver.clone(),
                            key_pointer,
                            format!("&{destination}"),
                        ],
                    );
                    let _ = writeln!(out, "    (void){call};");
                } else {
                    let fallback = self.materialize(out, &argument(2)?, value)?;
                    let call = self.emitter.runtime_call(
                        "void",
                        "subscript_rt_map_get_or",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "const void*".into(),
                            "void*".into(),
                        ],
                        &[
                            "ctx".into(),
                            receiver.clone(),
                            key_pointer,
                            fallback,
                            format!("&{destination}"),
                        ],
                    );
                    let _ = writeln!(out, "    {call};");
                }
            }
            "Set" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let value_pointer = self.materialize(out, &argument(2)?, value)?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_map_set",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        key_pointer,
                        value_pointer,
                        format!("{position}u"),
                    ],
                );
                let _ = writeln!(out, "    (void){call};");
                self.assign(out, result, receiver)?;
            }
            "Has" | "Delete" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let symbol = if name == "Has" {
                    "subscript_rt_assoc_has"
                } else {
                    "subscript_rt_assoc_delete"
                };
                let call = self.emitter.runtime_call(
                    "int32_t",
                    symbol,
                    &["void*".into(), "void*".into(), "const void*".into()],
                    &["ctx".into(), receiver.clone(), key_pointer],
                );
                self.assign(out, result, &format!("({call} != 0)"))?;
            }
            "Clear" => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_assoc_clear",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                let _ = writeln!(out, "    {call};");
            }
            "ForEach" => {
                let callback = argument(1)?;
                let bridge = self.emitter.define_assoc_bridge(key, Some(value))?;
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_map_for_each",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        format!("{callback}.code"),
                        format!("{callback}.env"),
                        format!("(const void*)&{bridge}"),
                    ],
                );
                let _ = writeln!(out, "    {call};");
            }
            other => return Err(internal(format!("unknown Map intrinsic {other}"))),
        }
        self.consume_runtime_traps(out, &instruction.traps, true, true)
    }

    /// Emits `new Set<K>(source)` as one fused runtime traversal
    /// (compiler.md §103.1 rule 4).
    pub(super) fn emit_set_from_source(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        spread: l::SpreadKind,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("Set source construction has no result"))?;
        let Type::Set(key) = data_type(self.value_type(result_id)?)?.clone() else {
            return Err(internal("Set source construction result is not a Set"));
        };
        let source = operands
            .first()
            .ok_or_else(|| internal("Set source construction has no operand"))?;
        let position = self.emitter.pos_id(&instruction.pos);
        let key_size = format!("(uint64_t)sizeof({})", self.emitter.ctype(&key)?);
        let key_kind = format!("{}u", association_key_kind(self.emitter.module, &key)?);
        let position = format!("{position}u");
        let call = match spread {
            l::SpreadKind::Array => self.emitter.runtime_call(
                "void*",
                "subscript_rt_set_from_array",
                &[
                    "void*".into(),
                    "void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &["ctx".into(), source.clone(), key_size, key_kind, position],
            ),
            l::SpreadKind::FixedArray => {
                let Some(l::ValueType::Data(Type::FixedArray(_, count))) = operand_types.first()
                else {
                    return Err(internal("Set fixed source type is invalid"));
                };
                self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_set_from_fixed",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "uint64_t".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("&({source})"),
                        format!("{count}ull"),
                        key_size,
                        key_kind,
                        position,
                    ],
                )
            }
            l::SpreadKind::SetValues => self.emitter.runtime_call(
                "void*",
                "subscript_rt_set_from_assoc",
                &[
                    "void*".into(),
                    "void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &["ctx".into(), source.clone(), key_size, key_kind, position],
            ),
            l::SpreadKind::StringCodePoints => self.emitter.runtime_call(
                "void*",
                "subscript_rt_set_from_string",
                &[
                    "void*".into(),
                    "const void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &["ctx".into(), source.clone(), key_size, key_kind, position],
            ),
        };
        self.assign(out, result, &call)?;
        self.consume_runtime_traps(out, &instruction.traps, true, true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_set_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        runtime_symbol: Option<&str>,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let key = if name == "New" {
            target.return_type.as_ref().and_then(|ty| match ty {
                l::ValueType::Data(Type::Set(key)) => Some(key.as_ref()),
                _ => None,
            })
        } else {
            operand_types.first().and_then(|ty| match ty {
                l::ValueType::Data(Type::Set(key)) => Some(key.as_ref()),
                _ => None,
            })
        }
        .ok_or_else(|| internal(format!("Set.{name} has no key type")))?;
        if name == "New" {
            let position = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_set_new",
                &[
                    "void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(key)?),
                    format!("{}u", association_key_kind(self.emitter.module, key)?),
                    format!("{position}u"),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true, true);
        }
        let receiver = operands
            .first()
            .ok_or_else(|| internal(format!("Set.{name} receiver is missing")))?;
        let argument = |index: usize| {
            operands
                .get(index)
                .cloned()
                .ok_or_else(|| internal(format!("Set.{name} operand {index} is missing")))
        };
        match name {
            "Size" => {
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_assoc_size",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                self.assign(out, result, &call)?;
            }
            "Add" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_set_add",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        key_pointer,
                        format!("{position}u"),
                    ],
                );
                let _ = writeln!(out, "    (void){call};");
                self.assign(out, result, receiver)?;
            }
            "Has" | "Delete" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let symbol = if name == "Has" {
                    "subscript_rt_assoc_has"
                } else {
                    "subscript_rt_assoc_delete"
                };
                let call = self.emitter.runtime_call(
                    "int32_t",
                    symbol,
                    &["void*".into(), "void*".into(), "const void*".into()],
                    &["ctx".into(), receiver.clone(), key_pointer],
                );
                self.assign(out, result, &format!("({call} != 0)"))?;
            }
            "Clear" => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_assoc_clear",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                let _ = writeln!(out, "    {call};");
            }
            "ForEach" => {
                let callback = argument(1)?;
                let bridge = self.emitter.define_assoc_bridge(key, None)?;
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_set_for_each",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        format!("{callback}.code"),
                        format!("{callback}.env"),
                        format!("(const void*)&{bridge}"),
                    ],
                );
                let _ = writeln!(out, "    {call};");
            }
            "Union" | "Intersection" | "Difference" | "SymmetricDifference" => {
                let symbol = runtime_symbol
                    .ok_or_else(|| internal(format!("Set.{name} has no runtime symbol")))?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        argument(1)?,
                        format!("{position}u"),
                    ],
                );
                self.assign(out, result, &call)?;
            }
            "IsSubsetOf" | "IsSupersetOf" | "IsDisjointFrom" => {
                let call = self.emitter.runtime_call(
                    "int32_t",
                    runtime_symbol
                        .ok_or_else(|| internal(format!("Set.{name} has no runtime symbol")))?,
                    &["void*".into(), "void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone(), argument(1)?],
                );
                self.assign(out, result, &format!("({call} != 0)"))?;
            }
            other => return Err(internal(format!("unknown Set intrinsic {other}"))),
        }
        self.consume_runtime_traps(out, &instruction.traps, true, true)
    }
}
