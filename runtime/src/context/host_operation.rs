use super::async_scheduler::{AsyncKind, RuntimeTask};
use super::*;
use crate::exception::{host_error::HostErrorLayout, Completion, ExceptionCompletion};
use std::sync::atomic::{AtomicU64, Ordering};

/// A host completion endpoint. Neither word is a native pointer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompletionEndpoint {
    /// Process-unique Context identity; zero identifies no Context.
    pub context_id: u64,
    /// Operation identity within one Context; zero identifies no operation.
    pub operation_id: u64,
}

/// The result of a host completion attempt (§178 rule 7).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompletionStatus {
    /// The source accepted the completion. Read the Context trap state after this result.
    Ok = 0,
    /// The endpoint belongs to another Context or an ended source.
    Stale = 1,
    /// The live source already holds its first completion.
    Duplicate = 2,
    /// The supplied result kind or size differs from the source.
    Mismatch = 3,
    /// The Context holds a trap. The source does not change.
    Trapped = 4,
}

pub(super) struct HostOperation {
    pub(super) operation_id: u64,
    is_void: bool,
    producer: bool,
    error_layout: HostErrorLayout,
}

pub(super) fn next_context_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .unwrap_or(0)
}

impl Context {
    pub(crate) fn host_operation_new(
        &mut self,
        size: usize,
        is_void: bool,
        pos_id: u32,
        error_layout: HostErrorLayout,
        endpoint: &mut CompletionEndpoint,
    ) -> *mut u8 {
        if self.trapped() {
            return std::ptr::null_mut();
        }
        if !error_layout.valid() || (is_void && size != 0) {
            self.trap(
                TrapKind::Internal,
                "invalid host operation metadata",
                pos_id,
            );
            return std::ptr::null_mut();
        }
        let Some(next_id) = self.next_host_operation_id.checked_add(1) else {
            self.trap(TrapKind::Internal, "host operation id exhausted", pos_id);
            return std::ptr::null_mut();
        };
        let operation_id = self.next_host_operation_id;
        self.next_host_operation_id = next_id;
        if self.host_operations.try_reserve(1).is_err() || self.async_frames.try_reserve(1).is_err()
        {
            self.trap(
                TrapKind::AllocationFailure,
                "host operation registry allocation failed",
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: RuntimeTask has a nonzero layout. A null allocation becomes a Context trap.
        let task_ptr =
            unsafe { std::alloc::alloc(Layout::new::<RuntimeTask>()) }.cast::<RuntimeTask>();
        if task_ptr.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                "host operation state allocation failed",
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: the fresh allocation has the exact Box layout and receives one initialized value.
        let task = unsafe {
            task_ptr.write(RuntimeTask::HostOperation(HostOperation {
                operation_id,
                is_void,
                producer: true,
                error_layout,
            }));
            Box::from_raw(task_ptr)
        };
        let handle = self.alloc(16, CLASS_GENERATOR, pos_id);
        if handle.is_null() {
            return handle;
        }
        unsafe {
            self.async_register(handle, size);
        }
        if self.trapped() {
            self.delete(handle as usize, pos_id);
            return std::ptr::null_mut();
        }
        unsafe {
            self.async_retain(handle);
        }
        if let Some(meta) = self.async_frames.get_mut(&(handle as usize)) {
            meta.create_pos_id = pos_id;
            meta.kind = AsyncKind::Runtime(task);
        }
        self.host_operations.insert(operation_id, handle as usize);
        *endpoint = CompletionEndpoint {
            context_id: self.context_id,
            operation_id,
        };
        handle
    }

    fn host_operation_lookup(
        &self,
        endpoint: CompletionEndpoint,
    ) -> Result<usize, CompletionStatus> {
        if self.trap_record().is_some() {
            return Err(CompletionStatus::Trapped);
        }
        if endpoint.context_id != self.context_id {
            return Err(CompletionStatus::Stale);
        }
        let handle = *self
            .host_operations
            .get(&endpoint.operation_id)
            .ok_or(CompletionStatus::Stale)?;
        let meta = self
            .async_frames
            .get(&handle)
            .ok_or(CompletionStatus::Stale)?;
        if meta.completion.is_some() {
            return Err(CompletionStatus::Duplicate);
        }
        Ok(handle)
    }

    pub(crate) unsafe fn host_complete_value(
        &mut self,
        endpoint: CompletionEndpoint,
        value: *const u8,
        size: usize,
        is_void: bool,
    ) -> CompletionStatus {
        let handle = match self.host_operation_lookup(endpoint) {
            Ok(h) => h,
            Err(s) => return s,
        };
        let Some(meta) = self.async_frames.get(&handle) else {
            return CompletionStatus::Stale;
        };
        let AsyncKind::Runtime(task) = &meta.kind else {
            return CompletionStatus::Stale;
        };
        let RuntimeTask::HostOperation(source) = task.as_ref() else {
            return CompletionStatus::Stale;
        };
        if source.is_void != is_void || meta.result_size != size {
            return CompletionStatus::Mismatch;
        }
        let pos_id = meta.create_pos_id;
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(size).is_err() {
            self.trap(
                TrapKind::AllocationFailure,
                "host completion allocation failed",
                pos_id,
            );
            return CompletionStatus::Trapped;
        }
        if size != 0 {
            // SAFETY: the C caller supplies readable value bytes after successful validation.
            bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(value, size) });
        }
        self.host_operation_finish(handle, Completion::Value(bytes));
        CompletionStatus::Ok
    }

    pub(crate) unsafe fn host_complete_error(
        &mut self,
        endpoint: CompletionEndpoint,
        message: *const u8,
        length: usize,
    ) -> CompletionStatus {
        let handle = match self.host_operation_lookup(endpoint) {
            Ok(h) => h,
            Err(s) => return s,
        };
        let Some(meta) = self.async_frames.get(&handle) else {
            return CompletionStatus::Stale;
        };
        let AsyncKind::Runtime(task) = &meta.kind else {
            return CompletionStatus::Stale;
        };
        let RuntimeTask::HostOperation(source) = task.as_ref() else {
            return CompletionStatus::Stale;
        };
        let (layout, pos_id) = (source.error_layout, meta.create_pos_id);
        let bytes = if length == 0 {
            &[]
        } else {
            // SAFETY: the C caller supplies readable message bytes after successful validation.
            unsafe { std::slice::from_raw_parts(message, length) }
        };
        let Some(exception) = self.host_error(layout, bytes, pos_id) else {
            return CompletionStatus::Trapped;
        };
        self.host_operation_finish(
            handle,
            Completion::Exception(Box::new(ExceptionCompletion {
                exception,
                observed: false,
            })),
        );
        CompletionStatus::Ok
    }

    fn host_operation_finish(&mut self, handle: usize, completion: Completion) {
        let Some(meta) = self.async_frames.get_mut(&handle) else {
            return;
        };
        let AsyncKind::Runtime(task) = &mut meta.kind else {
            return;
        };
        let RuntimeTask::HostOperation(source) = task.as_mut() else {
            return;
        };
        if !source.producer {
            return;
        }
        source.producer = false;
        let pos_id = meta.create_pos_id;
        meta.completion = Some(completion);
        self.async_ready.extend(std::mem::take(&mut meta.waiters));
        // SAFETY: the source transfers its one producer count exactly once.
        unsafe {
            self.async_release(handle as *mut u8, pos_id);
        }
    }
}
