//! Boundary marshalling of foreign values, structs, pointers, and arrays.

use super::*;

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
                self.marshal_boundary_struct(out, *class, value, position)
            }
            Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(class) if self.emitter.is_value_class(*class).unwrap_or(false)) =>
            {
                let Type::Class(class) = inner.as_ref() else {
                    unreachable!()
                };
                let (argument, writeback) =
                    self.marshal_boundary_pointer(out, *class, value, position, false)?;
                if let Some(writeback) = writeback {
                    writebacks.push(writeback);
                }
                Ok(argument)
            }
            _ => Ok(value.into()),
        }
    }

    fn marshal_boundary_struct(
        &mut self,
        out: &mut String,
        class: ClassId,
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
        let mut parts = Vec::new();
        let mut index = 0usize;
        while index < definition.fields.len() {
            let field = &definition.fields[index];
            let access = format!("{temporary}.d{}", field.id.0);
            match &field.ty {
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
                    let bind = self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_cb_bind",
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
                    parts.push(format!("({typedef_name})&subscript_rt_cb_trampoline"));
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
                    let (data, count) = match element.as_ref() {
                        Type::Class(element_class)
                            if self.emitter.is_value_class(*element_class)?
                                && boundary_class_requires_build(
                                    self.emitter.module,
                                    *element_class,
                                )? =>
                        {
                            self.marshal_boundary_array(
                                out,
                                *element_class,
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
                    parts.push(self.marshal_boundary_struct(out, *nested, &access, position)?);
                    index += 1;
                }
                Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(nested) if self.emitter.is_value_class(*nested).unwrap_or(false)) =>
                {
                    let Type::Class(nested) = inner.as_ref() else {
                        unreachable!()
                    };
                    let force_rebuild = !self.emitter.class(*nested)?.is_embedded_header;
                    parts.push(
                        self.marshal_boundary_pointer(
                            out,
                            *nested,
                            &access,
                            position,
                            force_rebuild,
                        )?
                        .0,
                    );
                    index += 1;
                }
                _ => {
                    parts.push(access);
                    index += 1;
                }
            }
        }
        Ok(format!(
            "(({}){{ {} }})",
            definition.source_name,
            parts.join(", ")
        ))
    }

    fn marshal_boundary_pointer(
        &mut self,
        out: &mut String,
        class: ClassId,
        pointer: &str,
        position: u32,
        force_rebuild: bool,
    ) -> Result<(String, Option<BoundaryPtrWriteback>), String> {
        if self.emitter.class(class)?.is_embedded_header
            || (!boundary_class_needs_scratch(self.emitter.module, class)?
                && !(force_rebuild && boundary_class_requires_build(self.emitter.module, class)?))
        {
            return Ok((
                format!("(({}*)({pointer}))", self.emitter.class(class)?.source_name),
                None,
            ));
        }
        let source = self.fresh();
        let language_type = self.emitter.class_name(class);
        let _ = writeln!(
            out,
            "    {language_type}* {source} = ({language_type}*)({pointer});"
        );
        let header = self.fresh();
        let header_type = self.emitter.class(class)?.source_name.clone();
        let _ = writeln!(
            out,
            "    {header_type}* {header} = NULL;\n    if ({source} != NULL) {{"
        );
        let allocation = self.emitter.runtime_call(
            "void*",
            "subscript_rt_boundary_scratch_alloc",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({header_type})"),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(
            out,
            "        {header} = ({header_type}*){allocation};\n        if (*(const uint32_t*)ctx != 0u) goto unwind;"
        );
        let value = self.marshal_boundary_struct(out, class, &format!("*{source}"), position)?;
        let _ = writeln!(out, "        *{header} = {value};\n    }}");
        Ok((
            header.clone(),
            Some(BoundaryPtrWriteback {
                class,
                source,
                scratch: header,
            }),
        ))
    }

    pub(super) fn marshal_boundary_array(
        &mut self,
        out: &mut String,
        element_class: ClassId,
        source: &str,
        count: &str,
        position: u32,
    ) -> Result<(String, String), String> {
        let language_type = self.emitter.class_name(element_class);
        let header_type = self.emitter.class(element_class)?.source_name.clone();
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
        let value = self.marshal_boundary_struct(
            out,
            element_class,
            &format!("{source_temporary}[{index}]"),
            position,
        )?;
        let _ = writeln!(out, "        {scratch}[{index}] = {value};\n    }}");
        Ok((scratch, count_temporary))
    }

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
            match &field.ty {
                Type::Array(_) | Type::Func(_) => {}
                Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(class) if self.emitter.is_value_class(*class)?) =>
                    {}
                Type::Str => {
                    let view = self.fresh();
                    let _ = writeln!(out, "        subscript_callback_string_view {view}; memcpy(&{view}, &{header}, sizeof {view});");
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
                    let _ = writeln!(out, "        {language} = {value};");
                }
                Type::Class(nested) if self.emitter.is_value_class(*nested)? => {
                    if !boundary_class_needs_scratch(self.emitter.module, *nested)? {
                        let _ = writeln!(
                            out,
                            "        memcpy(&{language}, &{header}, sizeof {language});"
                        );
                    }
                }
                _ => {
                    let _ = writeln!(out, "        {language} = {header};");
                }
            }
        }
        out.push_str("    }\n");
        Ok(())
    }
}
