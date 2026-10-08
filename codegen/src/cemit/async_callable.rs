//! Started handle producers for async function values (§167).
use super::*;

impl<'m> Emitter<'m> {
    pub(super) fn emit_async_callable(
        &mut self,
        out: &mut String,
        function: &l::Function,
    ) -> Result<(), String> {
        let signature = self.wrapper_signature(function)?;
        let mut arguments = vec!["ctx".to_string()];
        if matches!(function.kind, l::FunctionKind::Lambda) {
            arguments.push("environment".into());
        }
        arguments.extend(
            explicit_parameters(function).map(|parameter| format!("a{}", parameter.value.0)),
        );
        let _ = writeln!(
            out,
            "{signature} {{ (void)environment;\n    void* handle = sub_f{}({});",
            function.id.0,
            arguments.join(", ")
        );
        out.push_str("    if (*(const uint32_t*)ctx != 0u) return handle;\n");
        let (output, size) = if function.return_type == Type::Void {
            ("NULL".to_string(), "0u".to_string())
        } else {
            let _ = writeln!(
                out,
                "    {} value = {{0}};",
                self.completion_ctype(&function.return_type)?
            );
            ("&value".to_string(), "sizeof(value)".to_string())
        };
        let start = self.runtime_call(
            "uint8_t",
            "subscript_rt_async_start",
            &[
                "void*".into(),
                "void*".into(),
                "void*".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                "handle".into(),
                output.clone(),
                "create_pos".into(),
            ],
        );
        let _ = writeln!(out, "    uint8_t done = {start};");
        out.push_str("    if (*(const uint32_t*)ctx != 0u) return handle;\n");
        let complete = self.runtime_call(
            "void",
            "subscript_rt_async_complete",
            &[
                "void*".into(),
                "void*".into(),
                "const void*".into(),
                "uint64_t".into(),
            ],
            &["ctx".into(), "handle".into(), output, size],
        );
        let _ = writeln!(out, "    if (done) {complete};\n    return handle;\n}}\n");
        Ok(())
    }
}
