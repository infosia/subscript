//! Coroutine payload fields clear only at completion.

use super::*;

impl Body<'_, '_, '_> {
    pub(super) fn emit_finished_frame_clear(&self, out: &mut String) {
        #[cfg(test)]
        out.push_str("    // root-point 4294967294 0\n");
        // The header remains executable. Completion storage owns the returned value.
        out.push_str("    memset((unsigned char*)frame + sizeof frame->state + sizeof frame->reserved + sizeof frame->resume, 0, sizeof *frame - sizeof frame->state - sizeof frame->reserved - sizeof frame->resume);\n");
    }
}
