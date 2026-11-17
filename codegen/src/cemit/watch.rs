//! Inline finite pointer watches (compiler.md §325 rules 5, 8, and 9).
use super::*;
use crate::lower::watch::{self, Kind, Record};
use subscript_compiler::crossing::WatchPlan;

pub(super) struct Inline {
    storage: String,
    record: Record,
    indexed: bool,
    path: String,
    position: u32,
}
impl Body<'_, '_, '_> {
    pub(super) fn watch_memory(
        &mut self,
        out: &mut String,
        plan: &WatchPlan,
        name: &str,
        source: &str,
        extent: (&str, bool),
        position: u32,
    ) -> Result<(), String> {
        if self
            .boundary_targets
            .as_ref()
            .is_some_and(|targets| targets.elements != 0)
        {
            return Ok(());
        }
        if plan.recursive {
            return self.watch_recursive_memory(out, plan, name, source, extent, position);
        }
        let record = watch::record(self.emitter.module, &self.emitter.layouts, plan)?;
        let storage = self.fresh();
        let targets = self
            .boundary_targets
            .as_mut()
            .ok_or_else(|| internal("inline watch has no call"))?;
        let path = format!("{}.{name}", targets.callee);
        let words = if extent.1 {
            3
        } else {
            record
                .words
                .checked_add(1)
                .ok_or_else(|| internal("watch size overflow"))?
        };
        let initialized = u32::from(extent.1);
        let _ = writeln!(
            targets.declarations,
            "    uint64_t {storage}[{words}];\n    {storage}[{initialized}] = 0;"
        );
        if extent.1 {
            self.watch_capture_span(out, source, extent.0, &record, &storage, position)?;
        } else {
            let _ = writeln!(out, "    {storage}[0] = (uint64_t)(uintptr_t)({source});");
            self.watch_capture(out, source, &record, &format!("({storage} + 1u)"), position)?;
        }
        self.boundary_targets
            .as_mut()
            .ok_or_else(|| internal("inline watch lost its call"))?
            .inline_watches
            .push(Inline {
                storage,
                record,
                indexed: extent.1,
                path,
                position,
            });
        Ok(())
    }
    fn watch_load(&mut self, out: &mut String, source: &str) -> String {
        let value = self.fresh();
        let _ = writeln!(
            out,
            "    uint64_t {value}; memcpy(&{value}, (const void*)({source}), 8);"
        );
        value
    }
    fn watch_capture(
        &mut self,
        out: &mut String,
        source: &str,
        record: &Record,
        storage: &str,
        position: u32,
    ) -> Result<(), String> {
        let _ = writeln!(out, "    if ((uintptr_t)({source}) != 0u) {{");
        self.watch_capture_present(out, source, record, storage, position)?;
        out.push_str("    }\n");
        Ok(())
    }
    fn watch_capture_present(
        &mut self,
        out: &mut String,
        source: &str,
        record: &Record,
        storage: &str,
        position: u32,
    ) -> Result<(), String> {
        let mut values = std::collections::HashMap::new();
        for entry in &record.entries {
            let address = format!("((const unsigned char*)({source}) + {}u)", entry.offset);
            let target = format!("({storage} + {}u)", entry.word);
            match &entry.kind {
                Kind::Slot { .. } => {
                    let before = self.watch_load(out, &address);
                    let _ = writeln!(out, "    {target}[0] = {before};");
                    values.insert(entry.offset, before);
                }
                Kind::Pointer {
                    child,
                    child_word,
                    capture,
                } => {
                    let pointer = if *capture {
                        let value = self.watch_load(out, &address);
                        let _ = writeln!(out, "    {target}[0] = {value};");
                        value
                    } else {
                        values
                            .get(&entry.offset)
                            .cloned()
                            .ok_or_else(|| internal("watch pointer lost its saved slot"))?
                    };
                    self.watch_capture(
                        out,
                        &format!("(const void*)(uintptr_t){pointer}"),
                        child,
                        &format!("({storage} + {child_word}u)"),
                        position,
                    )?;
                }
                Kind::Embedded(child) => {
                    self.watch_capture_present(out, &address, child, &target, position)?
                }
                Kind::Array { element } => {
                    let array = self.watch_load(out, &address);
                    let _ = writeln!(out, "    if ({array} != 0u) {{");
                    let count = self.watch_load(out, &format!("(uintptr_t){array}"));
                    let data = self.watch_load(
                        out,
                        &format!("((const unsigned char*)(uintptr_t){array} + 24u)"),
                    );
                    self.watch_capture_span(
                        out,
                        &format!("(const void*)(uintptr_t){data}"),
                        &count,
                        element,
                        &target,
                        position,
                    )?;
                    let _ = writeln!(out, "    }} else {{ {target}[1] = 0; }}");
                }
            }
        }
        Ok(())
    }
    fn watch_capture_span(
        &mut self,
        out: &mut String,
        source: &str,
        count: &str,
        element: &Record,
        storage: &str,
        position: u32,
    ) -> Result<(), String> {
        // A span is evaluated once, before the buffer call and the element loop.
        let data = self.fresh();
        let length = self.fresh();
        let _ = writeln!(out, "    const unsigned char* const {data} = (const unsigned char*)({source});\n    const uint64_t {length} = (uint64_t)({count});");
        let buffer = self.fresh();
        let index = self.fresh();
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_boundary_watch_buffer",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("{length} * {}u", element.words),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(out, "    if ({data} != NULL && {length} != 0u) {{\n    uint64_t* __restrict {buffer} = {call};");
        self.scratch_allocation_check(out, "    ")?;
        let _ = writeln!(out, "    {storage}[0] = (uint64_t)(uintptr_t){buffer};\n    {storage}[1] = {length};\n    {storage}[2] = (uint64_t)(uintptr_t){data};\n    for (uint64_t {index} = 0; {index} < {length}; ++{index}) {{");
        self.watch_capture_present(
            out,
            &format!("({data} + {index} * {}u)", element.stride),
            element,
            &format!("({buffer} + {index} * {}u)", element.words),
            position,
        )?;
        let _ = writeln!(out, "    }}\n    }} else {{ {storage}[1] = 0; }}");
        Ok(())
    }
    pub(super) fn finish_inline_watches(
        &mut self,
        out: &mut String,
        watches: &[Inline],
    ) -> Result<(), String> {
        if watches.is_empty() {
            return Ok(());
        }
        let done = self.fresh();
        let _ = writeln!(out, "    if (*(const uint32_t*)ctx != 0u) goto {done};");
        for watch in watches {
            if watch.indexed {
                self.watch_compare_span(
                    out,
                    &watch.record,
                    &watch.storage,
                    &watch.path,
                    &[],
                    (watch.position, &done),
                )?;
            } else {
                self.watch_compare(
                    out,
                    &watch.record,
                    (
                        &format!("(const void*)(uintptr_t){}[0]", watch.storage),
                        &format!("({} + 1u)", watch.storage),
                    ),
                    &watch.path,
                    &[],
                    (watch.position, &done),
                )?;
            }
        }
        let _ = writeln!(out, "{done}: ;");
        Ok(())
    }
    fn watch_compare(
        &mut self,
        out: &mut String,
        record: &Record,
        place: (&str, &str),
        path: &str,
        indices: &[String],
        exit: (u32, &str),
    ) -> Result<(), String> {
        let _ = writeln!(out, "    if ((uintptr_t)({}) != 0u) {{", place.0);
        self.watch_compare_present(out, record, place, path, indices, exit)?;
        out.push_str("    }\n");
        Ok(())
    }
    fn watch_compare_present(
        &mut self,
        out: &mut String,
        record: &Record,
        place: (&str, &str),
        path: &str,
        indices: &[String],
        exit: (u32, &str),
    ) -> Result<(), String> {
        let (source, storage) = place;
        for entry in &record.entries {
            let address = format!("((const unsigned char*)({source}) + {}u)", entry.offset);
            let target = format!("({storage} + {}u)", entry.word);
            let path = format!("{path}.{}", entry.name);
            match &entry.kind {
                Kind::Slot { nullable } => {
                    let after = self.watch_load(out, &address);
                    let allow = if *nullable {
                        format!(" && {after} != 0u")
                    } else {
                        String::new()
                    };
                    let _ = writeln!(out, "    if ({after} != {target}[0]{allow}) {{");
                    self.watch_error(out, &path, indices, exit)?;
                    out.push_str("    }\n");
                }
                Kind::Pointer {
                    child, child_word, ..
                } => self.watch_compare(
                    out,
                    child,
                    (
                        &format!("(const void*)(uintptr_t){target}[0]"),
                        &format!("({storage} + {child_word}u)"),
                    ),
                    &path,
                    indices,
                    exit,
                )?,
                Kind::Embedded(child) => self.watch_compare_present(
                    out,
                    child,
                    (&address, &target),
                    &path,
                    indices,
                    exit,
                )?,
                Kind::Array { element } => {
                    self.watch_compare_span(out, element, &target, &path, indices, exit)?
                }
            }
        }
        Ok(())
    }
    fn watch_compare_span(
        &mut self,
        out: &mut String,
        element: &Record,
        storage: &str,
        path: &str,
        indices: &[String],
        exit: (u32, &str),
    ) -> Result<(), String> {
        let count = self.fresh();
        let buffer = self.fresh();
        let data = self.fresh();
        let index = self.fresh();
        let _ = writeln!(out, "    const uint64_t {count} = {storage}[1];\n    if ({count} != 0u) {{\n    const uint64_t* __restrict {buffer} = (const uint64_t*)(uintptr_t){storage}[0];\n    const unsigned char* const {data} = (const unsigned char*)(uintptr_t){storage}[2];\n    for (uint64_t {index} = 0; {index} < {count}; ++{index}) {{");
        let mut indices = indices.to_vec();
        indices.push(index.clone());
        self.watch_compare_present(
            out,
            element,
            (
                &format!("({data} + {index} * {}u)", element.stride),
                &format!("({buffer} + {index} * {}u)", element.words),
            ),
            &format!("{path}[]"),
            &indices,
            exit,
        )?;
        out.push_str("    }\n    }\n");
        Ok(())
    }
    fn watch_error(
        &mut self,
        out: &mut String,
        path: &str,
        indices: &[String],
        exit: (u32, &str),
    ) -> Result<(), String> {
        let (position, done) = exit;
        let label = self.emitter.language_string_pointer(path.as_bytes());
        let pointer = if indices.is_empty() {
            "NULL".into()
        } else {
            let name = self.fresh();
            let _ = writeln!(out, "    uint64_t {name}[] = {{{}}};", indices.join(", "));
            name
        };
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_boundary_watch_error",
            &[
                "void*".into(),
                "const unsigned char*".into(),
                "uint32_t".into(),
                "const uint64_t*".into(),
                "uint32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                label,
                format!("{}u", path.len()),
                pointer,
                format!("{}u", indices.len()),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(out, "    {call}; goto {done};");
        Ok(())
    }
}
