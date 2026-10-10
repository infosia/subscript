//! Script calls and foreign calls.

use super::*;
use subscript_compiler::crossing::ParameterPlan;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_call(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        match &target.kind {
            l::CallTargetKind::Function(function) => {
                self.emit_script_call(out, *function, operands, result)?;
                let check_pending =
                    script_call_requires_pending_check(self.emitter.function(*function)?);
                self.consume_runtime_traps(out, &instruction.traps, check_pending, false)
            }
            l::CallTargetKind::StaticClosure(function) => {
                let callable = operands
                    .first()
                    .ok_or_else(|| internal("static closure call has no callable"))?;
                let mut arguments = operands.iter().skip(1).cloned().collect::<Vec<_>>();
                if matches!(
                    target.return_type,
                    Some(l::ValueType::Data(Type::AsyncHandle(_)))
                ) {
                    arguments.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
                }
                let separator = if arguments.is_empty() { "" } else { ", " };
                let expression = format!(
                    "sub_{}{}(ctx, {callable}.env{separator}{})",
                    if self.emitter.function(*function)?.is_async {
                        "w"
                    } else {
                        "f"
                    },
                    function.0,
                    arguments.join(", ")
                );
                if let Some(result) = result {
                    let _ = writeln!(out, "    {result} = {expression};");
                } else {
                    let _ = writeln!(out, "    {expression};");
                }
                let check_pending =
                    script_call_requires_pending_check(self.emitter.function(*function)?);
                self.consume_runtime_traps(out, &instruction.traps, check_pending, false)
            }
            l::CallTargetKind::Method(method) => {
                let function = self.emitter.method_function(*method)?;
                self.emit_script_call(out, function, operands, result)?;
                let check_pending =
                    script_call_requires_pending_check(self.emitter.function(function)?);
                self.consume_runtime_traps(out, &instruction.traps, check_pending, false)
            }
            l::CallTargetKind::Indirect => {
                let callable = operands
                    .first()
                    .ok_or_else(|| internal("indirect call has no callable"))?;
                let l::ValueType::Data(Type::Func(signature)) = &operand_types[0] else {
                    return Err(internal("indirect call operand is not a function"));
                };
                let mut parameter_types = vec!["void*".to_string(), "void*".to_string()];
                parameter_types.extend(
                    signature
                        .params
                        .iter()
                        .map(|ty| self.emitter.ctype(ty))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                let mut args = operands.iter().skip(1).cloned().collect::<Vec<_>>();
                if matches!(&signature.ret, Type::AsyncHandle(_)) {
                    parameter_types.push("uint32_t".into());
                    args.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
                }
                let separator = if args.is_empty() { "" } else { ", " };
                let expression = format!(
                    "(({} (*)({}))({callable}.code))(ctx, {callable}.env{separator}{})",
                    self.emitter.ctype(&signature.ret)?,
                    parameter_types.join(", "),
                    args.join(", ")
                );
                if let Some(result) = result {
                    let _ = writeln!(out, "    {result} = {expression};");
                } else {
                    let _ = writeln!(out, "    {expression};");
                }
                self.consume_runtime_traps(out, &instruction.traps, true, false)
            }
            l::CallTargetKind::Foreign(function) => self.emit_foreign_call(
                out,
                instruction,
                *function,
                operands,
                operand_types,
                (result, None),
            ),
            l::CallTargetKind::Intrinsic(intrinsic) => self.emit_intrinsic(
                out,
                instruction,
                target,
                intrinsic,
                operands,
                operand_types,
                result,
            ),
            l::CallTargetKind::BuiltinMethod(method) => self.emit_builtin_method(
                out,
                instruction,
                target,
                *method,
                operands,
                operand_types,
                result,
            ),
        }
    }

    fn emit_script_call(
        &self,
        out: &mut String,
        function: l::FunctionId,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let separator = if operands.is_empty() { "" } else { ", " };
        let expression = format!("sub_f{}(ctx{separator}{})", function.0, operands.join(", "));
        if let Some(result) = result {
            let _ = writeln!(out, "    {result} = {expression};");
        } else {
            let _ = writeln!(out, "    {expression};");
        }
        Ok(())
    }

    pub(super) fn emit_foreign_call(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        function: l::ForeignFunctionId,
        operands: &[String],
        operand_types: &[l::ValueType],
        output: (Option<String>, Option<String>),
    ) -> Result<(), String> {
        let (result, endpoint) = output;
        let declaration = self
            .emitter
            .module
            .foreign_functions
            .get(function.0 as usize)
            .filter(|declaration| declaration.id == function)
            .cloned()
            .ok_or_else(|| internal(format!("foreign function {} is missing", function.0)))?;
        if !self
            .emitter
            .foreign_symbols
            .contains(&declaration.source_name)
        {
            self.emitter
                .foreign_symbols
                .push(declaration.source_name.clone());
        }
        // §189 rule 2: the call builds its marshaling from the plan of its
        // callee.
        let plan = self.crossing_plan(function)?;
        if plan.parameters.len() != declaration.parameters.len() {
            return Err(internal(format!(
                "foreign call `{}` has no plan for each parameter",
                declaration.source_name
            )));
        }
        // §187 rule 12: no result with a member that has no read lowering
        // reaches code generation.
        if let subscript_compiler::crossing::ResultPlan::Struct(read) = &plan.result {
            if let Some(unreadable) = &read.unreadable {
                return Err(internal(format!(
                    "foreign result of `{}` has a member with no read lowering: `{}.{}`",
                    declaration.source_name, unreadable.owner, unreadable.member
                )));
            }
        }
        // §187 rule 7: the scratch scope opens only for a call whose build
        // can allocate.
        let scratch_mark = if plan.scratch_scope {
            let mark = self.fresh();
            let call = self.emitter.runtime_call(
                "uint64_t",
                "subscript_rt_boundary_scratch_mark",
                &["void*".into()],
                &["ctx".into()],
            );
            let _ = writeln!(out, "    uint64_t {mark} = {call};");
            Some(mark)
        } else {
            None
        };
        let boundary_position = self.emitter.pos_id(&instruction.pos);
        self.boundary_targets = Some(BoundaryTargets::default());
        // The marshaling goes to its own buffer, so the variables of each
        // written-back target are declared before the first of them.
        let outer = out;
        let mut marshalled = String::new();
        let out = &mut marshalled;
        let mut arguments = Vec::new();
        let mut boundary_writebacks = Vec::new();
        let mut cursor = 0usize;
        for (parameter, parameter_plan) in declaration.parameters.iter().zip(&plan.parameters) {
            if *parameter_plan == ParameterPlan::CompletionEndpoint {
                arguments.push(
                    endpoint
                        .clone()
                        .ok_or_else(|| internal("completion endpoint is missing"))?,
                );
                continue;
            }
            if let ParameterPlan::Pair(pair) = parameter_plan {
                let Type::Array(element) = &parameter.ty else {
                    return Err(internal(format!(
                        "foreign call `{}` pair parameter `{}` is not an array",
                        declaration.source_name, parameter.source_name
                    )));
                };
                let data = operands.get(cursor).ok_or_else(|| {
                    internal(format!(
                        "foreign call `{}` array parameter `{}` has no data snapshot",
                        declaration.source_name, parameter.source_name
                    ))
                })?;
                let count = operands.get(cursor + 1).ok_or_else(|| {
                    internal(format!(
                        "foreign call `{}` array parameter `{}` has no count snapshot",
                        declaration.source_name, parameter.source_name
                    ))
                })?;
                let expected = l::ValueType::Address(l::AddressType {
                    pointee: element.as_ref().clone(),
                    array_base: None,
                });
                if operand_types.get(cursor) != Some(&expected)
                    || operand_types.get(cursor + 1) != Some(&l::ValueType::Data(Type::I32))
                {
                    return Err(internal(format!(
                        "foreign call `{}` array parameter `{}` snapshot types disagree with its declaration",
                        declaration.source_name, parameter.source_name
                    )));
                }
                let (data, count) = match &pair.elements {
                    Some(elements) => {
                        self.marshal_boundary_array(out, elements, data, count, boundary_position)?
                    }
                    None => (data.clone(), count.clone()),
                };
                match parameter.foreign_provenance.as_ref() {
                    Some(l::ForeignTypeProvenance::Descriptor {
                        aggregate,
                        element,
                        element_const,
                    }) => {
                        let pointer = if *element_const {
                            format!("(const {element}*)({data})")
                        } else {
                            format!("({element}*)({data})")
                        };
                        arguments
                            .push(format!("(({aggregate}){{ {pointer}, (size_t)({count}) }})"));
                    }
                    Some(l::ForeignTypeProvenance::ScalarPair {
                        element,
                        element_const,
                    }) => {
                        arguments.push(format!("(size_t)({count})"));
                        arguments.push(if *element_const {
                            format!("(const {element}*)({data})")
                        } else {
                            format!("({element}*)({data})")
                        });
                    }
                    other => {
                        return Err(internal(format!(
                            "foreign call `{}` array parameter `{}` has provenance {other:?}",
                            declaration.source_name, parameter.source_name
                        )))
                    }
                }
                cursor += 2;
                continue;
            }
            let value = operands.get(cursor).ok_or_else(|| {
                internal(format!(
                    "foreign parameter `{}` has no operand",
                    parameter.source_name
                ))
            })?;
            if operand_types.get(cursor).is_none_or(|ty| {
                !foreign_parameter_type_matches(self.emitter.module, ty, &parameter.ty)
            }) {
                return Err(internal(format!(
                    "foreign parameter `{}` operand type disagrees with its declaration",
                    parameter.source_name
                )));
            }
            arguments.push(
                self.marshal_foreign_value(
                    out,
                    parameter_plan,
                    parameter,
                    value,
                    boundary_position,
                    &mut boundary_writebacks,
                )
                .map_err(|error| {
                    format!(
                        "{error}; foreign emission site `{}.{}`",
                        declaration.source_name, parameter.source_name
                    )
                })?,
            );
            cursor += 1;
        }
        if cursor != operands.len() {
            return Err(internal(format!(
                "foreign call `{}` has inconsistent arity",
                declaration.source_name
            )));
        }
        let targets = self.boundary_targets.take().unwrap_or_default();
        let out = outer;
        out.push_str(&targets.declarations);
        out.push_str(&marshalled);
        let call = format!("{}({})", declaration.source_name, arguments.join(", "));
        match &declaration.return_type {
            ty if subscript_compiler::types::boundary_kind(ty)
                .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half) =>
            {
                self.assign(out, result.clone(), &format!("subscript_half_bits({call})"))?
            }
            Type::Void => {
                let _ = writeln!(out, "    {call};");
            }
            Type::Class(class) if self.emitter.is_value_class(*class)? => {
                let destination = result
                    .clone()
                    .ok_or_else(|| internal("foreign struct return has no result"))?;
                let header = self.fresh();
                let _ = writeln!(out, "    {} {header} = {call};\n    memcpy(&{destination}, &{header}, sizeof {destination});", self.emitter.class(*class)?.source_name);
            }
            _ => self.assign(out, result.clone(), &call)?,
        }
        if !boundary_writebacks.is_empty() || !targets.targets.is_empty() {
            out.push_str("    if (*(const uint32_t*)ctx == 0u) {\n");
            // The parameters first, then each nested target, the last one
            // built first.
            for writeback in boundary_writebacks
                .into_iter()
                .chain(targets.targets.into_iter().rev())
            {
                self.emit_boundary_writeback(out, writeback, boundary_position)?;
            }
            out.push_str("    }\n");
        }
        if let Some(mark) = scratch_mark {
            let release = self.emitter.runtime_call(
                "void",
                "subscript_rt_boundary_scratch_release",
                &["void*".into(), "uint64_t".into()],
                &["ctx".into(), mark],
            );
            let _ = writeln!(out, "    {release};");
        }
        let mut checked = false;
        for trap in &instruction.traps {
            match &trap.kind {
                l::TrapKind::Call | l::TrapKind::Allocation => {
                    self.consume(trap);
                    checked = true;
                }
                l::TrapKind::WireEnumValue(alias) => {
                    self.consume(trap);
                    let destination = result
                        .as_ref()
                        .ok_or_else(|| internal("wire-enum foreign return has no result"))?;
                    self.emit_wire_validation(out, destination, *alias, trap)?;
                }
                other => return Err(internal(format!("foreign call carries trap {other:?}"))),
            }
        }
        if checked {
            self.emit_pending_check(out);
        }
        Ok(())
    }
}
