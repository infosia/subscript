//! Replaced C members (compiler.md §325).
use super::*;
use subscript_compiler::crossing::{PointerPlan, WriteBack};

impl Body<'_, '_, '_> {
    pub(super) fn capture_input_arrays(
        &mut self,
        out: &mut String,
        plan: &subscript_compiler::crossing::WriteBackPlan,
        source: &str,
    ) -> Result<String, String> {
        let offsets = crate::lower::receive::input_arrays(&self.emitter.layouts, plan)?;
        let Some(last) = offsets.iter().max() else {
            return Ok("NULL".into());
        };
        let size = last
            .checked_add(8)
            .ok_or_else(|| internal("input array size overflow"))?;
        let original = self.fresh();
        let targets = self
            .boundary_targets
            .as_mut()
            .ok_or_else(|| internal("input array snapshot has no call"))?;
        let _ = writeln!(
            targets.declarations,
            "    unsigned char {original}[{size}];"
        );
        for offset in offsets {
            let _ = writeln!(out, "    memcpy({original} + {offset}u, (const unsigned char*)({source}) + {offset}u, 8);");
        }
        Ok(original)
    }

    pub(super) fn emit_received_member(
        &mut self,
        out: &mut String,
        write: &WriteBack,
        images: (&str, &str, &str),
        destination: (&str, &Type),
        label: &str,
        position: u32,
    ) -> Result<(), String> {
        let (source, snapshot, original) = images;
        let (destination, ty) = destination;
        let path = self.emitter.language_string_pointer(label.as_bytes());
        match write {
            WriteBack::Pointer { nullable } => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_boundary_pointer",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                        "const unsigned char*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("(const void*)(uintptr_t)({source})"),
                        u32::from(*nullable).to_string(),
                        path,
                        format!("{}u", label.len()),
                        format!("{position}u"),
                    ],
                );
                let zero = self.emitter.zero(&l::ValueType::Data(ty.clone()))?;
                let _ = writeln!(
                    out,
                    "            {call};
            {destination} = {zero};"
                );
            }
            WriteBack::Pair {
                pointer_first,
                element,
            } => {
                let schema = crate::lower::receive::schema(
                    self.emitter.module,
                    &self.emitter.layouts,
                    element,
                )?;
                let schema = self.emitter.language_string_pointer(&schema);
                let halves = self.fresh();
                let before = self.fresh();
                let earlier = self.fresh();
                let old_array = self.fresh();
                let (pointer, count) = if *pointer_first { (0, 1) } else { (1, 0) };
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_boundary_receive",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "uint64_t".into(),
                        "const unsigned char*".into(),
                        "const unsigned char*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                        "const uint64_t*".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("(const void*)(uintptr_t){halves}[{pointer}]"),
                        format!("{halves}[{count}]"),
                        schema,
                        path,
                        format!("{}u", label.len()),
                        format!("{position}u"),
                        earlier.clone(),
                    ],
                );
                let _ = writeln!(
                    out,
                    "            uint64_t {halves}[2]; memcpy({halves}, {source}, sizeof {halves});
            uint64_t {before}[2]; memcpy({before}, {snapshot}, sizeof {before});
            uint64_t {old_array}; memcpy(&{old_array}, {original}, 8);
            const uint64_t {earlier}[3] = {{ {before}[{pointer}], {before}[{count}], {old_array} }};
            {destination} = {call};"
                );
            }
            _ => return Err(internal("invalid received member")),
        }
        Ok(())
    }

    pub(super) fn snapshot_script_pointer(
        &mut self,
        out: &mut String,
        plan: &PointerPlan,
        pointer: &str,
        position: u32,
    ) -> Result<(), String> {
        let Some(watch) = &plan.watch else {
            return Ok(());
        };
        let name = plan.path.clone();
        self.watch_memory(out, watch, &name, pointer, ("1u", false), position)
    }

    pub(super) fn watch_recursive_memory(
        &mut self,
        out: &mut String,
        watch: &subscript_compiler::crossing::WatchPlan,
        name: &str,
        pointer: &str,
        extent: (&str, bool),
        position: u32,
    ) -> Result<(), String> {
        let (count, indexed) = extent;
        if self
            .boundary_targets
            .as_ref()
            .is_some_and(|targets| targets.elements != 0)
        {
            return Ok(());
        }
        let schema =
            crate::lower::receive::watch_schema(self.emitter.module, &self.emitter.layouts, watch)?;
        let schema = self.emitter.language_string_pointer(&schema);
        let targets = self
            .boundary_targets
            .as_ref()
            .ok_or_else(|| internal("pointer snapshot has no call"))?;
        let label = format!("{}.{name}", targets.callee);
        let path = self.emitter.language_string_pointer(label.as_bytes());
        let snapshot = self.fresh();
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_boundary_watch",
            &[
                "void*".into(),
                "void*".into(),
                "uint64_t".into(),
                "uint32_t".into(),
                "const unsigned char*".into(),
                "const unsigned char*".into(),
                "uint32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!("(void*)({pointer})"),
                format!("(uint64_t)({count})"),
                u32::from(indexed).to_string(),
                schema,
                path,
                format!("{}u", label.len()),
                format!("{position}u"),
            ],
        );
        let targets = self
            .boundary_targets
            .as_mut()
            .ok_or_else(|| internal("pointer snapshot has no call"))?;
        let _ = writeln!(targets.declarations, "    void* {snapshot} = NULL;");
        let _ = writeln!(out, "    {snapshot} = {call};");
        targets.watches.push(snapshot);
        self.scratch_allocation_check(out, "    ")?;
        Ok(())
    }
}
