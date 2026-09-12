//! Block terminators, boundary return stabilization, and edge copies.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_terminator(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        match &block.terminator {
            l::Terminator::Branch(target) => self.emit_edge(out, block.id, target),
            l::Terminator::ConditionalBranch {
                condition,
                then_target,
                else_target,
            } => {
                let condition = self.operand(condition)?;
                let next = self.fresh();
                let _ = writeln!(
                    out,
                    "    if ({condition}) goto {next}_then; else goto {next}_else;\n{next}_then:\n    ;"
                );
                self.emit_edge(out, block.id, then_target)?;
                let _ = writeln!(out, "{next}_else:\n    ;");
                self.emit_edge(out, block.id, else_target)
            }
            l::Terminator::Switch {
                value,
                arms,
                default,
            } => {
                let switch_type = self.operand_type(value)?;
                let value_type = data_type(&switch_type)?;
                let value = self.operand(value)?;
                let _ = writeln!(
                    out,
                    "    {{\n    {} _disc = {value};\n    switch (_disc) {{",
                    self.emitter.ctype(value_type)?
                );
                for arm in arms {
                    let constant = self.constant(&arm.value)?;
                    let _ = writeln!(out, "    case {constant}: ;");
                    self.emit_edge(out, block.id, &arm.target)?;
                }
                out.push_str("    default: ;\n");
                self.emit_edge(out, block.id, default)?;
                out.push_str("    }\n    }\n");
                Ok(())
            }
            l::Terminator::Return { value, pos: _ } => {
                let mut value = value
                    .as_ref()
                    .map(|value| self.operand(value))
                    .transpose()?;
                if self.coroutine {
                    if self.function.is_async {
                        if let Some(value) = value {
                            let ty = self.emitter.ctype(&self.function.return_type)?;
                            let _ = writeln!(out, "    *(({ty}*)coroutine_out) = {value};");
                        }
                    }
                    out.push_str("    frame->state = 0x7fffffff;\n");
                    self.emit_pop(out);
                    out.push_str("    return 1;\n");
                } else {
                    if let (Some(returned), Type::Class(class)) =
                        (value.as_ref(), &self.function.return_type)
                    {
                        if self.emitter.is_value_class(*class)?
                            && boundary_class_contains_pointer(self.emitter.module, *class)?
                        {
                            let stable = self.fresh();
                            let _ = writeln!(
                                out,
                                "    {} {stable} = {returned};",
                                self.emitter.class_name(*class)
                            );
                            self.emit_stabilize_boundary_return_value(
                                out,
                                *class,
                                &format!("&{stable}"),
                                &mut HashSet::new(),
                            )?;
                            value = Some(stable);
                        }
                    }
                    self.emit_pop(out);
                    if let Some(value) = value {
                        let _ = writeln!(out, "    return {value};");
                    } else {
                        out.push_str("    return;\n");
                    }
                }
                Ok(())
            }
            l::Terminator::Unreachable { .. } => {
                out.push_str("    goto unwind;\n");
                Ok(())
            }
            l::Terminator::Trap(trap) => {
                self.consume(trap);
                let pos = self.emitter.pos_id(&trap.pos);
                let kind = runtime_trap_kind(&trap.kind).ok_or_else(|| {
                    internal(format!("trap {:?} has no direct runtime kind", trap.kind))
                })? as u32;
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_trap",
                    &["void*".into(), "uint32_t".into(), "uint32_t".into()],
                    &["ctx".into(), format!("{kind}u"), format!("{pos}u")],
                );
                let _ = writeln!(out, "    {call};\n    goto unwind;");
                Ok(())
            }
            l::Terminator::Suspend { .. } => self.emit_suspend(out, block),
        }
    }

    fn emit_stabilize_boundary_return_value(
        &mut self,
        out: &mut String,
        class: ClassId,
        address: &str,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<(), String> {
        if !visiting.insert(class) {
            return Ok(());
        }
        let definition = self.emitter.class(class)?.clone();
        for field in &definition.fields {
            let field_address = format!("({address})->d{}", field.id.0);
            match &field.ty {
                Type::Nullable(inner) => {
                    let Type::Class(target) = inner.as_ref() else {
                        continue;
                    };
                    if !self.emitter.is_value_class(*target)? {
                        continue;
                    }
                    // The field already owns a Context-managed box. Its
                    // payload does not depend on the returning activation.
                }
                Type::Class(nested) if self.emitter.is_value_class(*nested)? => {
                    self.emit_stabilize_boundary_return_value(
                        out,
                        *nested,
                        &format!("&{field_address}"),
                        visiting,
                    )?;
                }
                Type::Array(element) => {
                    let Type::Class(element) = element.as_ref() else {
                        continue;
                    };
                    if !self.emitter.is_value_class(*element)? {
                        continue;
                    }
                    let element_type = self.emitter.class_name(*element);
                    let count = self.fresh();
                    let data = self.fresh();
                    let index = self.fresh();
                    let length = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_len",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), field_address.clone()],
                    );
                    let storage = self.emitter.runtime_call(
                        "const void*",
                        "subscript_rt_array_data",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), field_address],
                    );
                    let _ = writeln!(
                        out,
                        "    int32_t {count} = {length};\n    {element_type}* {data} = ({element_type}*){storage};\n    for (int32_t {index} = 0; {index} < {count}; {index}++) {{"
                    );
                    self.emit_stabilize_boundary_return_value(
                        out,
                        *element,
                        &format!("&{data}[{index}]"),
                        visiting,
                    )?;
                    out.push_str("    }\n");
                }
                _ => {}
            }
        }
        visiting.remove(&class);
        Ok(())
    }

    fn emit_edge(
        &mut self,
        out: &mut String,
        source: l::BlockId,
        target: &l::BlockTarget,
    ) -> Result<(), String> {
        let destination = &self.function.blocks[target.block.0 as usize];
        let mut copies = Vec::new();
        for (index, (argument, parameter)) in target
            .arguments
            .iter()
            .zip(&destination.parameters)
            .enumerate()
        {
            if self
                .removable_edge_copies
                .contains(&(source, target.block, index))
            {
                continue;
            }
            let destination = self.value_storage[parameter.0 as usize];
            let source = match argument {
                l::Operand::Value(value) => {
                    let value = self.value_storage[value.0 as usize];
                    if value == destination {
                        continue;
                    }
                    EdgeCopySource::Value(value)
                }
                l::Operand::Constant(constant) => EdgeCopySource::Constant(constant.clone()),
            };
            copies.push(EdgeCopy {
                destination,
                source,
            });
        }
        while !copies.is_empty() {
            if let Some(index) = copies.iter().position(|copy| {
                !copies.iter().any(|other| {
                    matches!(other.source, EdgeCopySource::Value(source) if source == copy.destination)
                })
            }) {
                let copy = copies.remove(index);
                let value = match copy.source {
                    EdgeCopySource::Value(value) => self.value(value),
                    EdgeCopySource::Constant(constant) => self.constant(&constant)?,
                    EdgeCopySource::Temporary(temporary) => temporary,
                };
                if self.is_function_value(copy.destination)? {
                    self.assign_function_value(out, copy.destination, &value)?;
                } else {
                    let _ = writeln!(out, "    {} = {value};", self.value(copy.destination));
                }
                continue;
            }

            let cycle_source = copies
                .iter()
                .find_map(|copy| match copy.source {
                    EdgeCopySource::Value(value) => Some(value),
                    EdgeCopySource::Constant(_) | EdgeCopySource::Temporary(_) => None,
                })
                .ok_or_else(|| internal("parallel edge copies could not make progress"))?;
            // One saved source breaks this cycle. Copies outside the cycle
            // remain pending and can expose another independent cycle.
            let source = self.value(cycle_source);
            let temporary = if self.is_function_value(cycle_source)?
                && self.emitter.has_closure_environments()
            {
                self.snapshot_function_value(out, &source)
            } else {
                let temporary = self.fresh();
                let _ = writeln!(
                    out,
                    "    {} {temporary} = {source};",
                    self.emitter.value_ctype(self.value_type(cycle_source)?)?
                );
                temporary
            };
            for copy in &mut copies {
                if matches!(copy.source, EdgeCopySource::Value(value) if value == cycle_source) {
                    copy.source = EdgeCopySource::Temporary(temporary.clone());
                }
            }
        }
        let _ = writeln!(out, "    goto b{};", target.block.0);
        Ok(())
    }
}
