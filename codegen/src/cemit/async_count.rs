//! Inline async handle counts (compiler.md §116.2 rule 6).

use super::*;

impl Body<'_, '_, '_> {
    pub(super) fn emit_async_count(
        &mut self,
        out: &mut String,
        handle: &str,
        release: Option<(u32, &[l::Trap])>,
    ) -> Result<(), String> {
        let slot = format!("*(uint32_t*)((uint8_t*){handle} + 4)");
        if let Some((pos, traps)) = release {
            let _ = writeln!(out, "    if ({handle} != NULL && {slot} > 1u) {{");
            let _ = writeln!(out, "        --({slot});");
            out.push_str("    } else {\n");
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_async_release",
                &["void*".into(), "void*".into(), "uint32_t".into()],
                &["ctx".into(), handle.into(), format!("{pos}u")],
            );
            let _ = writeln!(out, "    {call};");
            self.consume_runtime_traps(out, traps, true, false)?;
            out.push_str("    }\n");
        } else {
            let _ = writeln!(out, "    if ({handle} != NULL) {{");
            let _ = writeln!(out, "        if ({slot} != UINT32_MAX) ++({slot});");
            out.push_str("    }\n");
        }
        Ok(())
    }
}
