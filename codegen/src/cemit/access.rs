//! Allocation, boundary boxing, field and index addressing, and length.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_allocate_class(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        class: ClassId,
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("AllocateClass has no result"))?;
        if self.emitter.is_value_class(class)? {
            let result_id = instruction
                .result
                .ok_or_else(|| internal("allocation has no id"))?;
            if matches!(self.value_type(result_id)?, l::ValueType::Address(_)) {
                if self.coroutine && self.stable_values.contains(&result_id) {
                    let _ = writeln!(
                        out,
                        "    memset(&frame->stable_v{}, 0, sizeof frame->stable_v{});",
                        result_id.0, result_id.0
                    );
                    let _ = writeln!(out, "    {destination} = &frame->stable_v{};", result_id.0);
                } else {
                    let temporary = self.fresh();
                    let _ = writeln!(
                        out,
                        "    {} {temporary} = ({} ){{0}};",
                        self.emitter.class_name(class),
                        self.emitter.class_name(class)
                    );
                    self.assign(out, Some(destination), &format!("&{temporary}"))?;
                }
            } else {
                let zero = format!("({}){{0}}", self.emitter.class_name(class));
                self.assign(out, Some(destination), &zero)?;
            }
            for trap in &instruction.traps {
                self.consume(trap);
            }
            return Ok(());
        }
        let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
        let pos = self.emitter.pos_id(&trap.pos);
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_alloc",
            &[
                "void*".into(),
                "uint64_t".into(),
                "uint32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({})", self.emitter.class_name(class)),
                format!("{}u", class.0),
                format!("{pos}u"),
            ],
        );
        self.assign(out, Some(destination.clone()), &call)?;
        self.emit_pending_check(out);
        let class = &self.emitter.module.classes[class.0];
        let bytes = crate::counted::class_description(&self.emitter.layouts, class)?;
        if !bytes.is_empty() {
            let words = bytes
                .chunks_exact(8)
                .map(|word| {
                    let mut bytes = [0; 8];
                    bytes.copy_from_slice(word);
                    format!("{}ULL", u64::from_ne_bytes(bytes))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let name = format!(
                "object_description_{}",
                instruction
                    .result
                    .ok_or_else(|| internal("allocation has no id"))?
                    .0
            );
            let _ = writeln!(out, "    static const uint64_t {name}[] = {{{words}}};");
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_object_describe",
                &[
                    "void*".into(),
                    "void*".into(),
                    "const void*".into(),
                    "uint64_t".into(),
                ],
                &[
                    "ctx".into(),
                    destination,
                    name,
                    format!("{}ULL", bytes.len()),
                ],
            );
            let _ = writeln!(out, "    {call};");
        }
        Ok(())
    }

    pub(super) fn emit_box_boundary_value(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        payload: ClassId,
        operand: &str,
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("BoxBoundaryValue has no result"))?;
        let result_id = instruction
            .result
            .ok_or_else(|| internal("BoxBoundaryValue has no result id"))?;
        let result_type = self.value_type(result_id)?.clone();
        let l::ValueType::Data(Type::Nullable(inner)) = result_type else {
            return Err(internal("BoxBoundaryValue result is not nullable"));
        };
        let Type::Class(_) = inner.as_ref() else {
            return Err(internal("BoxBoundaryValue target is not a class"));
        };
        let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
        let pos = self.emitter.pos_id(&trap.pos);
        let class_name = self.emitter.class_name(payload);
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_alloc",
            &[
                "void*".into(),
                "uint64_t".into(),
                "uint32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({class_name})"),
                format!("{}u", payload.0),
                format!("{pos}u"),
            ],
        );
        self.assign(out, Some(destination.clone()), &call)?;
        self.emit_pending_check(out);
        let _ = writeln!(out, "    *(({class_name}*)({destination})) = {operand};");
        Ok(())
    }

    pub(super) fn emit_field_address(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        field: l::FieldRef,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        self.emit_generator_done_guard(out, instruction, operands, operand_types)?;
        let result_id = instruction
            .result
            .ok_or_else(|| internal("field address has no result"))?;
        if self.folded_addresses.contains(&result_id) {
            return Ok(());
        }
        let destination = result.ok_or_else(|| internal("field address has no destination"))?;
        let expression = match field {
            l::FieldRef::Class(field) => {
                let (class, _, _) = self.emitter.field(field)?;
                match &operand_types[0] {
                    l::ValueType::Address(_) => format!("&(({})->d{})", operands[0], field.0),
                    l::ValueType::Data(Type::Class(id)) if self.emitter.is_value_class(*id)? => {
                        format!("&(({}).d{})", operands[0], field.0)
                    }
                    l::ValueType::Data(Type::Nullable(inner)) if matches!(inner.as_ref(), Type::Class(id) if *id == class) =>
                    {
                        format!(
                            "&((({}*)({}))->d{})",
                            self.emitter.class_name(class),
                            operands[0],
                            field.0
                        )
                    }
                    l::ValueType::Data(Type::Class(_)) => format!(
                        "&((({}*)({}))->d{})",
                        self.emitter.class_name(class),
                        operands[0],
                        field.0
                    ),
                    other => return Err(internal(format!("field base is invalid: {other:?}"))),
                }
            }
            l::FieldRef::IterDone => match &operand_types[0] {
                l::ValueType::Address(_) => format!("&(({})->done)", operands[0]),
                _ => format!("&(({}).done)", operands[0]),
            },
            l::FieldRef::IterValue => match &operand_types[0] {
                l::ValueType::Address(_) => format!("&(({})->value)", operands[0]),
                _ => format!("&(({}).value)", operands[0]),
            },
        };
        let _ = writeln!(out, "    {destination} = {expression};");
        Ok(())
    }

    pub(super) fn emit_load_field(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        field: l::FieldRef,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if let l::FieldRef::Class(field_id) = field {
            let (class_id, _, _) = self.emitter.field(field_id)?;
            let expression = match &operand_types[0] {
                l::ValueType::Address(address) if matches!(&address.pointee, Type::Class(id) if self.emitter.is_value_class(*id)?) =>
                {
                    format!("({})->d{}", operands[0], field_id.0)
                }
                l::ValueType::Data(Type::Class(id)) if self.emitter.is_value_class(*id)? => {
                    format!("({}).d{}", operands[0], field_id.0)
                }
                l::ValueType::Data(Type::Nullable(inner)) if matches!(inner.as_ref(), Type::Class(id) if *id == class_id) =>
                {
                    format!(
                        "((({}*)({}))->d{})",
                        self.emitter.class_name(class_id),
                        operands[0],
                        field_id.0
                    )
                }
                l::ValueType::Data(Type::Class(_)) => format!(
                    "((({}*)({}))->d{})",
                    self.emitter.class_name(class_id),
                    operands[0],
                    field_id.0
                ),
                other => return Err(internal(format!("field load base is invalid: {other:?}"))),
            };
            for trap in &instruction.traps {
                match &trap.kind {
                    l::TrapKind::WireEnumValue(alias) => {
                        self.consume(trap);
                        self.emit_wire_validation(out, &expression, *alias, trap)?;
                    }
                    other => return Err(internal(format!("field load trap {other:?} is invalid"))),
                }
            }
            return self.assign(out, result, &expression);
        }
        self.emit_generator_done_guard(out, instruction, operands, operand_types)?;
        if field == l::FieldRef::IterValue
            && matches!(
                instruction
                    .result
                    .map(|value| self.value_type(value))
                    .transpose()?,
                Some(l::ValueType::Data(Type::Void))
            )
        {
            // A void loop binding has no value storage.
            return Ok(());
        }
        let expression = match field {
            l::FieldRef::IterDone => match &operand_types[0] {
                l::ValueType::Address(_) => format!("({})->done", operands[0]),
                _ => format!("({}).done", operands[0]),
            },
            l::FieldRef::IterValue => match &operand_types[0] {
                l::ValueType::Address(_) => format!("({})->value", operands[0]),
                _ => format!("({}).value", operands[0]),
            },
            l::FieldRef::Class(_) => unreachable!(),
        };
        self.assign(out, result, &expression)
    }

    fn emit_generator_done_guard(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
    ) -> Result<(), String> {
        for trap in &instruction.traps {
            if trap.kind == l::TrapKind::GeneratorDoneValue {
                self.consume(trap);
                let base = operands
                    .first()
                    .ok_or_else(|| internal("generator result is missing"))?;
                let condition = match instruction.operands.first() {
                    Some(l::Operand::Value(value)) if self.folded_addresses.contains(value) => {
                        let expression = self.folded_address_expression(*value)?;
                        format!("!({expression}).done")
                    }
                    _ => {
                        let member = match operand_types.first() {
                            Some(l::ValueType::Address(_)) => "->",
                            _ => ".",
                        };
                        format!("!({base}){member}done")
                    }
                };
                self.emit_guard(out, &condition, trap)?;
            }
        }
        Ok(())
    }

    pub(super) fn emit_wire_validation(
        &mut self,
        out: &mut String,
        value: &str,
        alias: subscript_compiler::types::StringAliasId,
        trap: &l::Trap,
    ) -> Result<(), String> {
        let definition = self
            .emitter
            .module
            .string_aliases
            .get(alias.0)
            .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
        let wires = definition
            .wire_values
            .as_ref()
            .ok_or_else(|| internal("wire validation targets a plain string alias"))?;
        let valid = wires
            .iter()
            .map(|wire| format!("({value}) == {wire}"))
            .collect::<Vec<_>>()
            .join(" || ");
        let pos = self.emitter.pos_id(&trap.pos);
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_trap_wire_enum",
            &[
                "void*".into(),
                "const unsigned char*".into(),
                "uint64_t".into(),
                "int32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!(
                    "(const unsigned char*){}",
                    c_string_literal(definition.source_name.as_bytes())
                ),
                format!("{}ull", definition.source_name.len()),
                value.into(),
                format!("{pos}u"),
            ],
        );
        let condition = if valid.is_empty() { "0" } else { &valid };
        let _ = writeln!(out, "    if (!({condition})) {{ {call}; goto unwind; }}");
        Ok(())
    }

    pub(super) fn emit_index_address(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        checked: bool,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("index address has no result"))?;
        let index = &operands[1];
        let (address, length) = match &operand_types[0] {
            l::ValueType::Data(Type::Array(element)) => {
                let header = self.fresh();
                let _ = writeln!(
                    out,
                    "    SsArrayHeader* {header} = (SsArrayHeader*)({});",
                    operands[0]
                );
                let length = checked.then(|| {
                    let length = self.fresh();
                    let _ = writeln!(out, "    uint64_t {length} = {header}->len;");
                    length
                });
                (
                    format!(
                        "({}*)({header}->data + (int64_t)({index}) * (int64_t)({header}->elem_size))",
                        self.emitter.ctype(element)?
                    ),
                    length,
                )
            }
            l::ValueType::Data(Type::FixedArray(_element, count)) => (
                format!("&((({}).a)[{index}])", operands[0]),
                checked.then(|| count.to_string()),
            ),
            l::ValueType::Address(address) => match &address.pointee {
                Type::FixedArray(_element, count) => (
                    format!("&((({})->a)[{index}])", operands[0]),
                    checked.then(|| count.to_string()),
                ),
                other => return Err(internal(format!("indexed address points to {other:?}"))),
            },
            other => return Err(internal(format!("indexed base is invalid: {other:?}"))),
        };
        if checked {
            let trap = instruction
                .traps
                .iter()
                .find(|trap| matches!(trap.kind, l::TrapKind::IndexRead | l::TrapKind::IndexWrite))
                .ok_or_else(|| internal("checked index has no bounds trap"))?
                .clone();
            self.consume(&trap);
            let length = length
                .as_ref()
                .ok_or_else(|| internal("checked index has no captured length"))?;
            let pos = self.emitter.pos_id(&trap.pos);
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_trap_index_out_of_bounds",
                &[
                    "void*".into(),
                    "int32_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    index.clone(),
                    format!("(uint32_t)({length})"),
                    format!("{pos}u"),
                ],
            );
            let _ = writeln!(
                out,
                "    if ((int64_t)({index}) < 0 || (uint64_t)({index}) >= (uint64_t)({length})) {{ {call}; goto unwind; }}"
            );
        }
        if !self.folded_addresses.contains(&result_id) {
            let destination = result.ok_or_else(|| internal("index address has no destination"))?;
            let _ = writeln!(out, "    {destination} = {address};");
        }
        Ok(())
    }

    pub(super) fn emit_length(
        &mut self,
        out: &mut String,
        operand: &str,
        operand_type: &l::ValueType,
        result: Option<String>,
    ) -> Result<(), String> {
        let expression = match operand_type {
            l::ValueType::Data(Type::Array(_)) => {
                format!("(int32_t)(((SsArrayHeader*)({operand}))->len)")
            }
            l::ValueType::Data(Type::Str) => self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_str_len",
                &["void*".into(), "const void*".into()],
                &["ctx".into(), operand.into()],
            ),
            l::ValueType::Data(Type::FixedArray(_, count)) => count.to_string(),
            other => return Err(internal(format!("length operand is invalid: {other:?}"))),
        };
        self.assign(out, result, &expression)
    }
}
