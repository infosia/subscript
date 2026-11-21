//! The host hooks and native libraries of the accept corpus entries, shared
//! by the corpus sweeps.

use subscript_codegen::NativeLibrary;

pub const HOST_OWNED_STATE_ID: &str = "a128-host-owned-state";
pub const HOST_OWNED_STATE_PRE_ENTRY: &str = "subHostOwnedStatePreEntry";
pub const HOST_OWNED_STATE_POST_RUN: &str = "subHostOwnedStatePostRun";
pub const HANDLE_ENTRY_PARAM_ID: &str = "a137-handle-entry-param";
pub const HANDLE_ENTRY_PARAM_PRE_ENTRY: &str = "subHostOwnedStateAdoptDrive";
pub const WIRE_ENTRY_PARAM_ID: &str = "a140-wire-entry-param";
pub const WIRE_ENTRY_PARAM_PRE_ENTRY: &str = "subWireEntryDrive";

/// The ship-tier pre-entry and post-run hooks of an accept entry that a
/// host drives through its exports.
pub fn host_hooks(id: &str) -> (Option<&'static str>, Option<&'static str>) {
    if id == HOST_OWNED_STATE_ID {
        (
            Some(HOST_OWNED_STATE_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
    } else if id == HANDLE_ENTRY_PARAM_ID {
        (
            Some(HANDLE_ENTRY_PARAM_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
    } else if id == WIRE_ENTRY_PARAM_ID {
        (Some(WIRE_ENTRY_PARAM_PRE_ENTRY), None)
    } else {
        (None, None)
    }
}

/// The native libraries of an accept entry, or `None` when the entry needs
/// the interop fixture and the fixture is not available here.
pub fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Option<Vec<NativeLibrary>> {
    if sources.iter().any(|source| {
        crate::corpus::references_interop(&source.source)
            || source.source.contains("SubWireMode")
            || source.source.contains("SubBindTone")
    }) {
        Some(vec![crate::native_fixture::fixture()?.library()])
    } else {
        Some(Vec::new())
    }
}
