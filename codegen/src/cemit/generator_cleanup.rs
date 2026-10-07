//! Static generator state cleanup uses C field offsets.

use super::*;

impl Emitter<'_> {
    pub(super) fn emit_generator_cleanup(
        &self,
        out: &mut String,
        function: &l::Function,
        state: Option<l::BlockId>,
        slots: &[(l::ValueId, String)],
    ) -> Result<(), String> {
        let fields = crate::generator_cleanup::fields(function, state)?;
        let mut words = vec![format!("{}ULL", fields.len())];
        for (value, ty) in fields {
            let field = slots
                .iter()
                .find_map(|(id, field)| (*id == value).then_some(field))
                .ok_or_else(|| {
                    internal(format!(
                        "generator cleanup value {} has no saved slot",
                        value.0
                    ))
                })?;
            let bytes = crate::counted::description(&self.layouts, ty)?;
            words.push(format!("offsetof(SubFrame{}, {field})", function.id.0));
            words.push(format!("{}ULL", bytes.len()));
            for word in bytes.chunks_exact(8) {
                let mut bytes = [0; 8];
                bytes.copy_from_slice(word);
                words.push(format!("{}ULL", u64::from_ne_bytes(bytes)));
            }
        }
        let suffix = state.map_or_else(|| "start".to_string(), |block| format!("b{}", block.0));
        let name = format!("sub_generator_cleanup_{}_{}", function.id.0, suffix);
        let _ = writeln!(
            out,
            "    static const uint64_t {name}[] = {{ {} }};\n    frame->cleanup = {name};",
            words.join(", ")
        );
        Ok(())
    }
}
