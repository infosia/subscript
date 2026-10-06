//! Resolved host task records (§169).
use subscript_compiler::Pos;

/// A registered task and its tier-local and resolved positions (§169).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AsyncTaskInfo {
    /// Context-local registration id, never reused.
    pub task_id: u64,
    /// Awaited task for a waiting invocation; otherwise zero.
    pub awaited_task_id: u64,
    /// READY=1, PARKED=2, WAITING=3, ACTIVE=4, COMPLETE=5, STOPPED=6.
    pub state: u32,
    /// Invocation=1, aggregate=2.
    pub kind: u32,
    /// Tier-local function allocation position id.
    pub function_pos_id: u32,
    /// Tier-local suspension position id, or zero.
    pub await_pos_id: u32,
    /// Tier-local call position id, or zero.
    pub create_pos_id: u32,
    /// Reserved; always zero.
    pub reserved: u32,
    /// Resolved call position; empty for host roots and aggregates.
    pub create_pos: Pos,
    /// Resolved function allocation position.
    pub function_pos: Pos,
    /// Resolved suspension position; empty when no script site exists.
    pub await_pos: Pos,
}

pub(crate) unsafe extern "C" fn collect(
    data: *mut std::ffi::c_void,
    info: *const subscript_runtime::AsyncTaskInfo,
) {
    // SAFETY: ReloadSession supplies a matching vector and a live record.
    unsafe {
        (&mut *data.cast::<Vec<subscript_runtime::AsyncTaskInfo>>()).push(*info);
    }
}
