//! Boundary marshaling of foreign values, structs, pointers, and arrays.
//!
//! The marshaling builds each crossing from the plan of its callee
//! ([`subscript_compiler::crossing::call_plan`], §189 rule 2). The plan
//! carries every pass decision of the boundary crate (§187 rule 3) and the
//! members that the call builds and writes back, so this module walks the
//! plan and never the members. The call writes back the scratch copy of
//! each non-`const` pointer (§187 rule 7) by code after the call: a pointer
//! parameter, and each pointer target in [`BoundaryTargets`]. No pair of
//! written-back elements reaches code generation (rule 11). Each such
//! scratch copy has a snapshot of the bytes that the call put there, in
//! the same storage, and the copy-back writes a member back only when C
//! changed it.

use std::sync::Arc;

use super::*;
use subscript_boundary::{PointerPass, StructPass};
use subscript_compiler::crossing::{
    CallPlan, ElementsPlan, MemberCrossing, NotLowered, ParameterPlan, PointerPlan, StructPlan,
    WriteBack,
};

/// True when `ty` is an `f16`, whose C value converts at the boundary.
fn is_half(ty: &Type) -> bool {
    subscript_compiler::types::boundary_kind(ty)
        .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half)
}

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    /// The plan of `callee`, built on its first call.
    pub(super) fn crossing_plan(
        &mut self,
        callee: l::ForeignFunctionId,
    ) -> Result<Arc<CallPlan>, String> {
        let module = self.emitter.module;
        self.emitter.crossing_plans.get(module, callee)
    }

    pub(super) fn marshal_foreign_value(
        &mut self,
        out: &mut String,
        plan: &ParameterPlan,
        parameter: &l::ForeignParameter,
        value: &str,
        position: u32,
        writebacks: &mut Vec<BoundaryPtrWriteback>,
    ) -> Result<String, String> {
        match plan {
            ParameterPlan::Value if is_half(&parameter.ty) => {
                Ok(format!("subscript_half_native({value})"))
            }
            ParameterPlan::Value => Ok(value.into()),
            ParameterPlan::StringView => {
                let Some(l::ForeignTypeProvenance::StringView { aggregate }) =
                    parameter.foreign_provenance.as_ref()
                else {
                    return Err(internal(
                        "foreign string parameter has no string-view provenance",
                    ));
                };
                let data = self.emitter.runtime_call(
                    "const void*",
                    "subscript_rt_str_data",
                    &["const void*".into(), "const void*".into()],
                    &["ctx".into(), value.into()],
                );
                let len = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_str_len",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), value.into()],
                );
                Ok(format!(
                    "(({aggregate}){{ (const char*)({data}), (size_t)({len}) }})"
                ))
            }
            // §187 rule 10: a struct that copies its bytes passes them.
            ParameterPlan::ByValue(structure) => {
                self.marshal_boundary_struct(out, structure, value, position)
            }
            ParameterPlan::Pointer(pointer) => {
                let (argument, writeback) =
                    self.marshal_boundary_pointer(out, pointer, value, position, true)?;
                if let Some(writeback) = writeback {
                    writebacks.push(writeback);
                }
                Ok(argument)
            }
            ParameterPlan::Pair(_) | ParameterPlan::CompletionEndpoint => Err(internal(
                "a pair or a completion endpoint reached the value marshaling",
            )),
            other => Err(internal(format!("foreign parameter plan {other:?}"))),
        }
    }

    /// Builds the C value of the struct of `plan`.
    fn marshal_boundary_struct(
        &mut self,
        out: &mut String,
        plan: &StructPlan,
        value: &str,
        position: u32,
    ) -> Result<String, String> {
        let class = plan.class;
        let definition = self.emitter.class(class)?.clone();
        let temporary = self.fresh();
        let _ = writeln!(
            out,
            "    {} {temporary} = {value};",
            self.emitter.class_name(class)
        );
        match plan.pass {
            // The C bytes are the script bytes, a fixed array of scalars
            // included (§187 rule 10).
            StructPass::Bytes => {
                return Ok(format!("(*({}*)&{temporary})", definition.source_name))
            }
            StructPass::Cycle => return Err(crate::lower::struct_cycle(&definition.source_name)),
            StructPass::Scratch => {}
        }
        let mut parts = Vec::new();
        let mut arrays = Vec::new();
        for member in &plan.members {
            let field = definition
                .fields
                .get(member.field)
                .ok_or_else(|| internal("a boundary plan member has no field"))?;
            let access = format!("{temporary}.d{}", field.id.0);
            match &member.crossing {
                MemberCrossing::Bytes if is_half(&field.ty) => {
                    parts.push(format!("subscript_half_native({access})"));
                }
                MemberCrossing::Bytes => parts.push(access),
                MemberCrossing::Callback(callback) => {
                    let Some(l::ForeignTypeProvenance::Callback { typedef_name }) =
                        field.foreign_provenance.as_ref()
                    else {
                        return Err(internal(format!(
                            "boundary callback field `{}.{}` has no typedef provenance",
                            definition.source_name, field.source_name
                        )));
                    };
                    let userdata = definition
                        .fields
                        .get(callback.userdata)
                        .ok_or_else(|| internal("boundary callback has no userdata field"))?;
                    let second = callback
                        .second
                        .map(|index| {
                            definition
                                .fields
                                .get(index)
                                .ok_or_else(|| internal("boundary callback has no second slot"))
                        })
                        .transpose()?;
                    // §111 rule 2: the plan reads the lifetime off the class.
                    // It never derives it from the source name.
                    let explicit = callback.explicit;
                    let (crossing, trampoline) = if explicit {
                        (
                            "subscript_rt_cb_register",
                            "subscript_rt_cb_registration_trampoline",
                        )
                    } else {
                        ("subscript_rt_cb_bind", "subscript_rt_cb_trampoline")
                    };
                    if explicit {
                        // The generated code only takes this address, so
                        // no call declares it. The preamble declares the
                        // Context-lifetime trampoline for every program
                        // with a foreign function; this one is declared
                        // where it is used, so a program without an
                        // explicit-lifetime crossing is unchanged.
                        self.emitter.declare_runtime(
                            "void",
                            trampoline,
                            &[
                                "subscript_callback_string_view".into(),
                                "void*".into(),
                                "void*".into(),
                            ],
                        );
                    }
                    let bind = self.emitter.runtime_call(
                        "void*",
                        crossing,
                        &[
                            "void*".into(),
                            "const void*".into(),
                            "const void*".into(),
                            "void*".into(),
                            "void*".into(),
                        ],
                        &[
                            "ctx".into(),
                            format!("{access}.code"),
                            format!("{access}.env"),
                            format!("{temporary}.d{}", userdata.id.0),
                            second.map_or_else(
                                || "NULL".into(),
                                |field| format!("{temporary}.d{}", field.id.0),
                            ),
                        ],
                    );
                    parts.push(format!("({typedef_name})&{trampoline}"));
                    parts.push(bind);
                    if second.is_some() {
                        parts.push("NULL".into());
                    }
                }
                MemberCrossing::Pair(pair) => {
                    let count_call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_len",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), access.clone()],
                    );
                    let data_call = self.emitter.runtime_call(
                        "const void*",
                        "subscript_rt_array_data",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), access],
                    );
                    let (data, count) = match &pair.elements {
                        Some(elements) => self.marshal_boundary_array(
                            out,
                            elements,
                            &data_call,
                            &count_call,
                            position,
                        )?,
                        None => (data_call, count_call),
                    };
                    parts.push(format!("(size_t)({count})"));
                    parts.push(format!("(void*)({data})"));
                }
                MemberCrossing::StringView => {
                    let data = self.emitter.runtime_call(
                        "const void*",
                        "subscript_rt_str_data",
                        &["const void*".into(), "const void*".into()],
                        &["ctx".into(), access.clone()],
                    );
                    let len = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_str_len",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), access],
                    );
                    parts.push(format!("{{ (const char*)({data}), (size_t)({len}) }}"));
                }
                MemberCrossing::Embedded(nested) => {
                    parts.push(self.marshal_boundary_struct(out, nested, &access, position)?);
                }
                MemberCrossing::Pointer(pointer) => {
                    // The copy-back keeps the script link, and the call
                    // writes back the scratch copy of a non-`const` target
                    // (§187 rule 7).
                    let (pointer, writeback) =
                        self.marshal_boundary_pointer(out, pointer, &access, position, false)?;
                    if let Some(writeback) = writeback {
                        self.boundary_targets
                            .as_mut()
                            .ok_or_else(|| internal("a written-back target has no call"))?
                            .targets
                            .push(writeback);
                    }
                    parts.push(pointer);
                }
                // §187 rule 13: a fixed array of scalars copies as bytes
                // after the struct is built.
                MemberCrossing::FixedBytes => {
                    parts.push("{0}".into());
                    arrays.push((field.source_name.clone(), access));
                }
                MemberCrossing::NotLowered(NotLowered::FixedArrayOfStructs) => {
                    return Err(crate::lower::fixed_array_member(
                        &definition.source_name,
                        &field.source_name,
                    ));
                }
                MemberCrossing::NotLowered(NotLowered::CallbackWithoutUserdata) => {
                    return Err(internal("boundary callback has no userdata field"));
                }
                other => {
                    return Err(internal(format!(
                        "boundary field `{}.{}` has the plan {other:?}",
                        definition.source_name, field.source_name
                    )))
                }
            }
        }
        let built = format!("(({}){{ {} }})", definition.source_name, parts.join(", "));
        if arrays.is_empty() {
            return Ok(built);
        }
        let value = self.fresh();
        let _ = writeln!(out, "    {} {value} = {built};", definition.source_name);
        for (member, access) in arrays {
            let _ = writeln!(
                out,
                "    memcpy({value}.{member}, &{access}, sizeof {value}.{member});"
            );
        }
        Ok(value)
    }

    /// Passes the target of a struct pointer as `plan` says. The writeback
    /// is `Some` only for [`PointerPass::ScratchWrittenBack`]: its storage
    /// holds the scratch struct and then its snapshot. The target of a
    /// parameter (`local`) is a local of the call; any other target is
    /// scratch memory. A written-back target declares its variables at the
    /// scope of the call, so the call writes it back after the call.
    fn marshal_boundary_pointer(
        &mut self,
        out: &mut String,
        plan: &PointerPlan,
        pointer: &str,
        position: u32,
        local: bool,
    ) -> Result<(String, Option<BoundaryPtrWriteback>), String> {
        let class = plan.class;
        let written_back = match plan.pass {
            PointerPass::ScriptMemory => {
                return Ok((
                    format!("(({}*)({pointer}))", self.emitter.class(class)?.source_name),
                    None,
                ))
            }
            PointerPass::Cycle => {
                return Err(crate::lower::struct_cycle(
                    &self.emitter.class(class)?.source_name,
                ))
            }
            PointerPass::ScratchWrittenBack => true,
            PointerPass::ScratchReadOnly => false,
        };
        let target = plan
            .target
            .as_ref()
            .ok_or_else(|| internal("a scratch pointer has no target plan"))?;
        let copies = if written_back { 2 } else { 1 };
        let source = self.fresh();
        let header = self.fresh();
        let snapshot = self.fresh();
        let language_type = self.emitter.class_name(class);
        let header_type = self.emitter.class(class)?.source_name.clone();
        if written_back {
            let targets = self
                .boundary_targets
                .as_mut()
                .ok_or_else(|| internal("a written-back target has no call"))?;
            if targets.elements != 0 {
                return Err(crate::lower::written_back_in_elements(&header_type));
            }
            let _ = writeln!(
                targets.declarations,
                "    {language_type}* {source} = NULL;\n    {header_type}* {header} = NULL;\n    {header_type}* {snapshot} = NULL;"
            );
            let _ = writeln!(out, "    {source} = ({language_type}*)({pointer});");
        } else {
            let _ = writeln!(
                out,
                "    {language_type}* {source} = ({language_type}*)({pointer});\n    {header_type}* {header} = NULL;"
            );
        }
        let storage = if local {
            let storage = self.fresh();
            let _ = writeln!(out, "    {header_type} {storage}[{copies}];");
            storage
        } else {
            self.emitter.runtime_call(
                "void*",
                "subscript_rt_boundary_scratch_alloc",
                &["void*".into(), "uint64_t".into(), "uint32_t".into()],
                &[
                    "ctx".into(),
                    format!("(uint64_t)({copies}u * sizeof({header_type}))"),
                    format!("{position}u"),
                ],
            )
        };
        let _ = writeln!(
            out,
            "    if ({source} != NULL) {{\n        {header} = ({header_type}*){storage};"
        );
        if !local {
            out.push_str("        if (*(const uint32_t*)ctx != 0u) goto unwind;\n");
        }
        let value = self.marshal_boundary_struct(out, target, &format!("*{source}"), position)?;
        let _ = writeln!(out, "        *{header} = {value};");
        if written_back {
            let _ = writeln!(
                out,
                "        {snapshot} = {header} + 1; memcpy({snapshot}, {header}, sizeof({header_type}));"
            );
        }
        out.push_str("    }\n");
        let writeback = if written_back {
            Some(BoundaryPtrWriteback {
                plan: plan
                    .write_back
                    .clone()
                    .ok_or_else(|| internal("a written-back target has no write-back plan"))?,
                source,
                scratch: header.clone(),
                snapshot,
            })
        } else {
            None
        };
        Ok((header, writeback))
    }

    /// Builds a scratch array of the `count` script elements at `source`,
    /// which the call passes as `plan` says. The binder and the checker
    /// reject a pair whose elements the call writes back (§187 rule 11), so
    /// the pass is [`PointerPass::ScratchReadOnly`].
    pub(super) fn marshal_boundary_array(
        &mut self,
        out: &mut String,
        plan: &ElementsPlan,
        source: &str,
        count: &str,
        position: u32,
    ) -> Result<(String, String), String> {
        let element_class = plan.class;
        let language_type = self.emitter.class_name(element_class);
        let header_type = self.emitter.class(element_class)?.source_name.clone();
        match plan.pass {
            PointerPass::ScratchReadOnly => {}
            PointerPass::ScratchWrittenBack => {
                return Err(crate::lower::written_back_elements(&header_type))
            }
            PointerPass::Cycle => return Err(crate::lower::struct_cycle(&header_type)),
            PointerPass::ScriptMemory => {
                return Err(internal(format!(
                    "the elements of `{header_type}` copy their bytes and need no scratch array"
                )))
            }
        }
        let element = plan
            .element
            .as_ref()
            .ok_or_else(|| internal("a scratch array has no element plan"))?;
        let source_temporary = self.fresh();
        let count_temporary = self.fresh();
        let scratch = self.fresh();
        let index = self.fresh();
        let _ = writeln!(
            out,
            "    size_t {count_temporary} = (size_t)({count});\n    const {language_type}* {source_temporary} = (const {language_type}*)({source});"
        );
        let allocation = self.emitter.runtime_call(
            "void*",
            "subscript_rt_boundary_scratch_alloc",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("(uint64_t)({count_temporary} * sizeof({header_type}))"),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(
            out,
            "    {header_type}* {scratch} = ({header_type}*){allocation};\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    for (size_t {index} = 0; {index} < {count_temporary}; {index}++) {{"
        );
        // §187 rule 11: no build inside the loop writes back a target.
        self.element_depth(1)?;
        let value = self.marshal_boundary_struct(
            out,
            element,
            &format!("{source_temporary}[{index}]"),
            position,
        );
        self.element_depth(-1)?;
        let value = value?;
        let _ = writeln!(out, "        {scratch}[{index}] = {value};\n    }}");
        Ok((scratch, count_temporary))
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

    /// Writes back the members of one scratch struct that C changed (§187
    /// rules 6 and 7): each member that the write-back plan names is
    /// written only when its scratch bytes differ from the snapshot that
    /// the call took after it built the struct. A script write that a
    /// callback makes during the call stays when C leaves the member
    /// unchanged.
    pub(super) fn emit_boundary_writeback(
        &mut self,
        out: &mut String,
        writeback: BoundaryPtrWriteback,
        position: u32,
    ) -> Result<(), String> {
        let class = self.emitter.class(writeback.plan.class)?.clone();
        let _ = writeln!(out, "    if ({} != NULL) {{", writeback.source);
        for member in &writeback.plan.members {
            let field = class
                .fields
                .get(member.field)
                .ok_or_else(|| internal("a write-back plan member has no field"))?;
            let language = format!("{}->d{}", writeback.source, field.id.0);
            let header = format!("{}->{}", writeback.scratch, field.source_name);
            let before = format!("{}->{}", writeback.snapshot, field.source_name);
            let changed = format!("memcmp(&{header}, &{before}, sizeof {header}) != 0");
            match &member.write {
                WriteBack::Bytes if is_half(&field.ty) => {
                    let _ = writeln!(
                        out,
                        "        if ({changed}) {language} = subscript_half_bits({header});"
                    );
                }
                WriteBack::FixedBytes => {
                    let _ = writeln!(
                        out,
                        "        if ({changed}) memcpy(&{language}, &{header}, sizeof {header});"
                    );
                }
                WriteBack::Bytes => {
                    let _ = writeln!(out, "        if ({changed}) {language} = {header};");
                }
                WriteBack::StringView => {
                    // A view that C left as the call built it keeps the
                    // script string: the copy-back allocates only for a
                    // view that C wrote.
                    let view = self.fresh();
                    let value = self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_str_from_view",
                        &[
                            "void*".into(),
                            "const unsigned char*".into(),
                            "uint64_t".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            format!("{view}.data"),
                            format!("(uint64_t){view}.len"),
                            format!("{position}u"),
                        ],
                    );
                    let _ = writeln!(
                        out,
                        "        if ({changed}) {{ subscript_callback_string_view {view}; memcpy(&{view}, &{header}, sizeof {view}); {language} = {value}; }}"
                    );
                }
                WriteBack::EmbeddedBytes(_) => {
                    let _ = writeln!(
                        out,
                        "        if ({changed}) memcpy(&{language}, &{header}, sizeof {language});"
                    );
                }
                WriteBack::Embedded(nested) => self.emit_boundary_writeback(
                    out,
                    BoundaryPtrWriteback {
                        plan: nested.clone(),
                        source: format!("(&{language})"),
                        scratch: format!("(&{header})"),
                        snapshot: format!("(&{before})"),
                    },
                    position,
                )?,
                WriteBack::Cycle(nested) => {
                    return Err(crate::lower::struct_cycle(
                        &self.emitter.class(*nested)?.source_name,
                    ))
                }
                other => return Err(internal(format!("boundary write-back plan {other:?}"))),
            }
        }
        out.push_str("    }\n");
        Ok(())
    }
}
