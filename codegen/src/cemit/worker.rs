//! Context byte intrinsics and worker intrinsics.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_context_bytes(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        _target: &l::CallTarget,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[String],
        _operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let ty = intrinsic
            .type_argument
            .as_ref()
            .ok_or_else(|| internal(format!("Context.{name} has no type argument")))?;
        let ctype = self.emitter.ctype(ty)?;
        let pos = self.emitter.pos_id(&instruction.pos);
        match name {
            "BytesOf" => {
                let source = operands
                    .first()
                    .ok_or_else(|| internal("Context.BytesOf value is missing"))?;
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_from_bytes",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("&{source}"),
                        format!("(uint32_t)sizeof({ctype})"),
                        format!("{pos}u"),
                    ],
                );
                let destination =
                    result.ok_or_else(|| internal("Context.BytesOf has no result"))?;
                let _ = writeln!(out, "    {destination} = {call};");
                self.emit_pending_check(out);
                let data = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_data",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), destination],
                );
                emit_padding_zero(self.emitter, out, &data, ty)?;
            }
            "BytesInto" => {
                let source = operands
                    .first()
                    .ok_or_else(|| internal("Context.BytesInto value is missing"))?;
                let target = operands
                    .get(1)
                    .ok_or_else(|| internal("Context.BytesInto target is missing"))?;
                let offset = operands
                    .get(2)
                    .ok_or_else(|| internal("Context.BytesInto offset is missing"))?;
                let range = self.fresh();
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_byte_range",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        target.clone(),
                        offset.clone(),
                        format!("(uint32_t)sizeof({ctype})"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    void* {range} = {call};\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    memcpy({range}, &{source}, sizeof({ctype}));");
                emit_padding_zero(self.emitter, out, &range, ty)?;
            }
            "FromBytes" => {
                let bytes = operands
                    .first()
                    .ok_or_else(|| internal("Context.FromBytes source is missing"))?;
                let offset = operands
                    .get(1)
                    .ok_or_else(|| internal("Context.FromBytes offset is missing"))?;
                let destination =
                    result.ok_or_else(|| internal("Context.FromBytes has no result"))?;
                let range = self.fresh();
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_byte_range",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        bytes.clone(),
                        offset.clone(),
                        format!("(uint32_t)sizeof({ctype})"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    void* {range} = {call};\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    memcpy(&{destination}, {range}, sizeof({ctype}));");
            }
            other => return Err(internal(format!("unknown Context byte intrinsic {other}"))),
        }
        self.consume_runtime_traps(out, &instruction.traps, false, true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_worker_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        intrinsic: &l::Intrinsic,
        name: &str,
        runtime_symbol: Option<&str>,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if name == "Spawn" {
            let index = intrinsic
                .worker_entry
                .ok_or_else(|| internal("Worker.Spawn has no worker entry"))?
                as usize;
            let entry = self
                .emitter
                .module
                .worker_entries
                .get(index)
                .ok_or_else(|| internal(format!("worker entry {index} is missing")))?;
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_worker_spawn",
                &[
                    "subscript_rt_context*".into(),
                    "subscript_rt_worker_init".into(),
                    "subscript_rt_worker_entry".into(),
                    "const subscript_rt_worker_message_descriptor*".into(),
                    "const subscript_rt_worker_message_descriptor*".into(),
                ],
                &[
                    "ctx".into(),
                    "subscript_init".into(),
                    format!("subscript_worker_entry{index}"),
                    format!("&sub_worker_message_descriptor_{}", entry.input.0),
                    format!("&sub_worker_message_descriptor_{}", entry.output.0),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true, true);
        }
        self.emit_simple_runtime_intrinsic(
            out,
            instruction,
            target,
            runtime_symbol
                .ok_or_else(|| internal(format!("Worker.{name} has no runtime symbol")))?,
            operands,
            operand_types,
            false,
            result,
        )
    }
}
