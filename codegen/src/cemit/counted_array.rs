//! Counted array mutations and copies use a static element description.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_counted_array_operation(
        &mut self,
        out: &mut String,
        array: &str,
        operation: u32,
        value: &str,
        target: &str,
        start: &str,
        end: &str,
        instruction: &l::Instruction,
    ) -> Result<(), String> {
        let element = instruction
            .count_action
            .as_ref()
            .and_then(l::CountAction::release_type);
        let bytes = if let Some(element) = element {
            crate::counted::description(&self.emitter.layouts, &element)?
        } else if operation == 3 && instruction.count_action == Some(l::CountAction::Uncounted) {
            Vec::new()
        } else {
            return Err(internal("counted array operation has no count action"));
        };
        let description = bytes
            .chunks_exact(8)
            .map(|word| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(word);
                format!("{}ULL", u64::from_ne_bytes(bytes))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let position = self.emitter.pos_id(&instruction.pos);
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_counted_array_operation",
            &[
                "void*".into(),
                "void*".into(),
                "const void*".into(),
                "uint32_t".into(),
                "const void*".into(),
                "int32_t".into(),
                "int32_t".into(),
                "int32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                array.into(),
                if bytes.is_empty() {
                    "NULL".into()
                } else {
                    format!("(const uint64_t[]){{{description}}}")
                },
                format!("{operation}u"),
                value.into(),
                target.into(),
                start.into(),
                end.into(),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(out, "    {call};");
        Ok(())
    }
}
