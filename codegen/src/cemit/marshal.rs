//! Boundary marshalling of foreign values, structs, pointers, and arrays.
//!
//! Every pass decision comes from the boundary crate (§187 rule 3):
//! [`subscript_boundary::parameter_pass`], [`subscript_boundary::member_pass`],
//! [`subscript_boundary::element_pass`], and [`subscript_boundary::copy_back`].
//! The call writes back the scratch copy of each non-`const` pointer
//! (§187 rule 7) by code after the call: a pointer parameter, and each
//! pointer target in [`BoundaryTargets`]. No pair of written-back elements
//! reaches code generation (rule 11). Each such scratch copy has a
//! snapshot of the bytes that the call put there, in the same storage, and
//! the copy-back writes a member back only when C changed it.

use super::*;
use subscript_boundary::{CopyBack, PointerPass, StructPass};

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn marshal_foreign_value(
        &mut self,
        out: &mut String,
        ty: &Type,
        provenance: Option<&l::ForeignTypeProvenance>,
        value: &str,
        position: u32,
        writebacks: &mut Vec<BoundaryPtrWriteback>,
    ) -> Result<String, String> {
        match ty {
            ty if subscript_compiler::types::boundary_kind(ty)
                .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half) =>
            {
                Ok(format!("subscript_half_native({value})"))
            }
            Type::Str => {
                let Some(l::ForeignTypeProvenance::StringView { aggregate }) = provenance else {
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
            Type::Class(class) if self.emitter.is_value_class(*class)? => {
                // §187 rule 10: a struct that copies its bytes passes them.
                let pass = subscript_boundary::value_parameter_pass(&self.boundary_view(), *class);
                self.marshal_boundary_struct(out, *class, pass, value, position)
            }
            Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(class) if self.emitter.is_value_class(*class).unwrap_or(false)) =>
            {
                let Type::Class(class) = inner.as_ref() else {
                    unreachable!()
                };
                // §187 rule 7: the call does not write a `const` target
                // back.
                let writable = provenance != Some(&l::ForeignTypeProvenance::ConstPointer);
                let pass =
                    subscript_boundary::parameter_pass(&self.boundary_view(), *class, writable);
                let (argument, writeback) =
                    self.marshal_boundary_pointer(out, *class, pass, value, position, true)?;
                if let Some(writeback) = writeback {
                    writebacks.push(writeback);
                }
                Ok(argument)
            }
            _ => Ok(value.into()),
        }
    }

    /// The LIR class view that the pass decision reads.
    pub(super) fn boundary_view(
        &self,
    ) -> subscript_compiler::boundary_pass::Classes<'e, l::Module> {
        crate::lir::boundary_view(self.emitter.module)
    }

    /// True when the call passes the elements of a pair of `element` as a
    /// scratch array.
    pub(super) fn elements_need_scratch(&self, element: &Type) -> Result<bool, String> {
        Ok(match element {
            Type::Class(class) if self.emitter.is_value_class(*class)? => {
                subscript_boundary::struct_pass(&self.boundary_view(), *class) != StructPass::Bytes
            }
            _ => false,
        })
    }

    /// The pass of the elements of a pair of `element` structs, whose
    /// element pointer is `const` when `writable` is false.
    pub(super) fn element_pass(&self, element: ClassId, writable: bool) -> PointerPass {
        subscript_boundary::element_pass(&self.boundary_view(), element, writable)
    }

    /// Builds the C value of `class`, which the call passes as `pass`.
    fn marshal_boundary_struct(
        &mut self,
        out: &mut String,
        class: ClassId,
        pass: StructPass,
        value: &str,
        position: u32,
    ) -> Result<String, String> {
        let definition = self.emitter.class(class)?.clone();
        let temporary = self.fresh();
        let _ = writeln!(
            out,
            "    {} {temporary} = {value};",
            self.emitter.class_name(class)
        );
        match pass {
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
        let mut index = 0usize;
        while index < definition.fields.len() {
            let field = &definition.fields[index];
            let access = format!("{temporary}.d{}", field.id.0);
            match &field.ty {
                ty if subscript_compiler::types::boundary_kind(ty)
                    .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half) =>
                {
                    parts.push(format!("subscript_half_native({access})"));
                    index += 1;
                }
                Type::Func(_) => {
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
                        .get(index + 1)
                        .ok_or_else(|| internal("boundary callback has no userdata field"))?;
                    let second = definition
                        .fields
                        .get(index + 2)
                        .filter(|field| is_userdata_slot(&field.ty));
                    // §111 rule 2: the emitter reads the lifetime off the
                    // class. It never derives it from the source name.
                    let explicit = definition.callback_lifetime == CallbackLifetime::Explicit;
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
                        index += 3;
                    } else {
                        index += 2;
                    }
                }
                Type::Array(element) => {
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
                    let writable =
                        field.foreign_provenance != Some(l::ForeignTypeProvenance::ConstPointer);
                    let (data, count) = match element.as_ref() {
                        Type::Class(element_class) if self.elements_need_scratch(element)? => {
                            let elements = self.element_pass(*element_class, writable);
                            self.marshal_boundary_array(
                                out,
                                *element_class,
                                elements,
                                &data_call,
                                &count_call,
                                position,
                            )?
                        }
                        _ => (data_call, count_call),
                    };
                    parts.push(format!("(size_t)({count})"));
                    parts.push(format!("(void*)({data})"));
                    index += 1;
                }
                Type::Str => {
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
                    index += 1;
                }
                Type::Class(nested) if self.emitter.is_value_class(*nested)? => {
                    let nested_pass =
                        subscript_boundary::embedded_pass(&self.boundary_view(), pass, *nested);
                    parts.push(self.marshal_boundary_struct(
                        out,
                        *nested,
                        nested_pass,
                        &access,
                        position,
                    )?);
                    index += 1;
                }
                Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(nested) if self.emitter.is_value_class(*nested).unwrap_or(false)) =>
                {
                    let Type::Class(nested) = inner.as_ref() else {
                        unreachable!()
                    };
                    // The copy-back keeps the script link, and the call
                    // writes back the scratch copy of a non-`const` target
                    // (§187 rule 7).
                    let writable =
                        field.foreign_provenance != Some(l::ForeignTypeProvenance::ConstPointer);
                    let member = subscript_boundary::member_pass(
                        &self.boundary_view(),
                        pass,
                        *nested,
                        writable,
                    );
                    let (pointer, writeback) = self
                        .marshal_boundary_pointer(out, *nested, member, &access, position, false)?;
                    if let Some(writeback) = writeback {
                        self.boundary_targets
                            .as_mut()
                            .ok_or_else(|| internal("a written-back target has no call"))?
                            .targets
                            .push(writeback);
                    }
                    parts.push(pointer);
                    index += 1;
                }
                // §187 rule 13: a fixed array of scalars copies as bytes
                // after the struct is built.
                ty @ Type::FixedArray(..)
                    if crate::lower::is_scalar_fixed_array(self.emitter.module, ty) =>
                {
                    parts.push("{0}".into());
                    arrays.push((field.source_name.clone(), access));
                    index += 1;
                }
                Type::FixedArray(..) => {
                    return Err(crate::lower::fixed_array_member(
                        &definition.source_name,
                        &field.source_name,
                    ));
                }
                _ => {
                    parts.push(access);
                    index += 1;
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

    /// Passes the target of a struct pointer as `pass` says. The writeback
    /// is `Some` only for [`PointerPass::ScratchWrittenBack`]: its storage
    /// holds the scratch struct and then its snapshot. The target of a
    /// parameter (`local`) is a local of the call; any other target is
    /// scratch memory. A written-back target declares its variables at the
    /// scope of the call, so the call writes it back after the call.
    fn marshal_boundary_pointer(
        &mut self,
        out: &mut String,
        class: ClassId,
        pass: PointerPass,
        pointer: &str,
        position: u32,
        local: bool,
    ) -> Result<(String, Option<BoundaryPtrWriteback>), String> {
        let written_back = match pass {
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
        let value = self.marshal_boundary_struct(
            out,
            class,
            StructPass::Scratch,
            &format!("*{source}"),
            position,
        )?;
        let _ = writeln!(out, "        *{header} = {value};");
        if written_back {
            let _ = writeln!(
                out,
                "        {snapshot} = {header} + 1; memcpy({snapshot}, {header}, sizeof({header_type}));"
            );
        }
        out.push_str("    }\n");
        let writeback = written_back.then(|| BoundaryPtrWriteback {
            class,
            source,
            scratch: header.clone(),
            snapshot,
        });
        Ok((header, writeback))
    }

    /// Builds a scratch array of the `count` script elements at `source`,
    /// which the call passes as `pass`. The binder and the checker reject a
    /// pair whose elements the call writes back (§187 rule 11), so `pass`
    /// is [`PointerPass::ScratchReadOnly`].
    pub(super) fn marshal_boundary_array(
        &mut self,
        out: &mut String,
        element_class: ClassId,
        pass: PointerPass,
        source: &str,
        count: &str,
        position: u32,
    ) -> Result<(String, String), String> {
        let language_type = self.emitter.class_name(element_class);
        let header_type = self.emitter.class(element_class)?.source_name.clone();
        match pass {
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
            element_class,
            StructPass::Scratch,
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
    /// rules 6 and 7): each member that [`subscript_boundary::copy_back`]
    /// names is written only when its scratch bytes differ from the
    /// snapshot that the call took after it built the struct. A script
    /// write that a callback makes during the call stays when C leaves the
    /// member unchanged.
    pub(super) fn emit_boundary_writeback(
        &mut self,
        out: &mut String,
        writeback: BoundaryPtrWriteback,
        position: u32,
    ) -> Result<(), String> {
        let class = self.emitter.class(writeback.class)?.clone();
        let _ = writeln!(out, "    if ({} != NULL) {{", writeback.source);
        for field in &class.fields {
            let language = format!("{}->d{}", writeback.source, field.id.0);
            let header = format!("{}->{}", writeback.scratch, field.source_name);
            let before = format!("{}->{}", writeback.snapshot, field.source_name);
            let changed = format!("memcmp(&{header}, &{before}, sizeof {header}) != 0");
            let shape =
                subscript_compiler::boundary_pass::field_shape(self.emitter.module, &field.ty);
            match subscript_boundary::copy_back(&shape) {
                CopyBack::Skip => {}
                CopyBack::Bytes
                    if subscript_compiler::types::boundary_kind(&field.ty)
                        .is_some_and(|kind| kind.leaf == subscript_boundary::Leaf::Half) =>
                {
                    let _ = writeln!(
                        out,
                        "        if ({changed}) {language} = subscript_half_bits({header});"
                    );
                }
                CopyBack::Bytes if matches!(field.ty, Type::FixedArray(..)) => {
                    let _ = writeln!(
                        out,
                        "        if ({changed}) memcpy(&{language}, &{header}, sizeof {header});"
                    );
                }
                CopyBack::Bytes => {
                    let _ = writeln!(out, "        if ({changed}) {language} = {header};");
                }
                CopyBack::StringView => {
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
                CopyBack::Embedded(nested) => {
                    match subscript_boundary::struct_pass(&self.boundary_view(), nested) {
                        StructPass::Bytes => {
                            let _ = writeln!(
                                out,
                                "        if ({changed}) memcpy(&{language}, &{header}, sizeof {language});"
                            );
                        }
                        StructPass::Scratch => self.emit_boundary_writeback(
                            out,
                            BoundaryPtrWriteback {
                                class: nested,
                                source: format!("(&{language})"),
                                scratch: format!("(&{header})"),
                                snapshot: format!("(&{before})"),
                            },
                            position,
                        )?,
                        StructPass::Cycle => {
                            return Err(crate::lower::struct_cycle(
                                &self.emitter.class(nested)?.source_name,
                            ))
                        }
                    }
                }
            }
        }
        out.push_str("    }\n");
        Ok(())
    }
}
