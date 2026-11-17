//! Snapshots of reachable script-memory pointer slots (compiler.md §325 rule 5).
use super::receive::{address, fault, label, word};
use crate::context::Context;
use std::collections::HashSet;

struct Slot {
    address: *mut *const u8,
    before: *const u8,
    member: *const u8,
    visit: usize,
}
struct Visit {
    source: *mut u8,
    node: usize,
    parent: Option<usize>,
    member: *const u8,
    element: Option<usize>,
}
#[derive(Default)]
struct Snapshot {
    slots: Vec<Slot>,
    nodes: Vec<*const u8>,
    visits: Vec<Visit>,
    seen: HashSet<(usize, usize)>,
    path: *const u8,
    length: u32,
    position: u32,
}

/// Reusable call snapshots. A scope retains its storage across calls.
#[derive(Default)]
pub(crate) struct WatchPool {
    // Each FFI handle must stay at its address when another snapshot enters the pool.
    #[allow(clippy::vec_box)]
    snapshots: Vec<Box<Snapshot>>,
    used: usize,
    buffers: Vec<Vec<u64>>,
    buffers_used: usize,
}
impl WatchPool {
    pub(crate) fn mark(&self) -> (usize, usize) {
        (self.used, self.buffers_used)
    }
    pub(crate) fn release(&mut self, mark: (usize, usize)) {
        self.used = mark.0;
        self.buffers_used = mark.1;
    }
    fn words(&mut self, words: usize) -> Result<*mut u64, ()> {
        if self.buffers_used == self.buffers.len() {
            self.buffers.try_reserve(1).map_err(|_| ())?;
            self.buffers.push(Vec::new());
        }
        let buffer = &mut self.buffers[self.buffers_used];
        if words > buffer.len() {
            buffer
                .try_reserve_exact(words - buffer.len())
                .map_err(|_| ())?;
            buffer.resize(words, 0);
        }
        self.buffers_used += 1;
        Ok(buffer.as_mut_ptr())
    }
    fn acquire(&mut self) -> &mut Snapshot {
        if self.used == self.snapshots.len() {
            self.snapshots.push(Box::default());
        }
        let snapshot = &mut self.snapshots[self.used];
        self.used += 1;
        snapshot.slots.clear();
        snapshot.nodes.clear();
        snapshot.visits.clear();
        snapshot.seen.clear();
        snapshot
    }
}
impl Snapshot {
    fn visit(&mut self, visit: Visit) {
        if !visit.source.is_null() && self.seen.insert((visit.source as usize, visit.node)) {
            self.visits.push(visit);
        }
    }
    unsafe fn error_path(&self, slot: &Slot) -> String {
        let mut chain = Vec::new();
        let mut next = Some(slot.visit);
        while let Some(index) = next {
            chain.push(index);
            next = self.visits[index].parent;
        }
        let mut path = unsafe { label(self.path, self.length) };
        for index in chain.into_iter().rev() {
            let visit = &self.visits[index];
            if !visit.member.is_null() {
                path.push('.');
                path.push_str(&unsafe { label(visit.member.add(16), word(visit.member, 12)) });
            }
            if let Some(element) = visit.element {
                use std::fmt::Write;
                let _ = write!(path, "[{element}]");
            }
        }
        path.push('.');
        path.push_str(&unsafe { label(slot.member.add(16), word(slot.member, 12)) });
        path
    }
}

/// Returns reusable words for a finite array snapshot. The call scope releases the buffer.
///
/// # Safety
/// Shared FFI contract. The caller writes each snapshot word before it reads that word.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_watch_buffer(
    ctx: *mut Context,
    words: u64,
    position: u32,
) -> *mut u64 {
    let ctx = unsafe { &mut *ctx };
    let result = usize::try_from(words)
        .ok()
        .filter(|words| words.checked_mul(8).is_some())
        .ok_or(())
        .and_then(|words| ctx.boundary_watch.words(words));
    match result {
        Ok(pointer) => pointer,
        Err(()) => {
            ctx.trap(
                crate::trap::TrapKind::AllocationFailure,
                "boundary watch buffer allocation failed",
                position,
            );
            std::ptr::null_mut()
        }
    }
}

/// Reports one inline watch mismatch. Only this error path builds the indexed member text.
///
/// # Safety
/// Shared FFI contract. `path` contains `length` UTF-8 bytes; `indices` has `count` words.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_watch_error(
    ctx: *mut Context,
    path: *const u8,
    length: u32,
    indices: *const u64,
    count: u32,
    position: u32,
) {
    use std::fmt::Write;
    let template = unsafe { label(path, length) };
    let mut text = String::new();
    let mut parts = template.split("[]");
    text.push_str(parts.next().unwrap_or_default());
    for (index, part) in parts.enumerate() {
        if index < count as usize {
            let value = unsafe { indices.add(index).read_unaligned() };
            let _ = write!(text, "[{value}]");
        }
        text.push_str(part);
    }
    fault(
        unsafe { &mut *ctx },
        &text,
        "C replaced a pointer that the script cannot receive",
        position,
    );
}

/// Captures C-writable slots before a call, with cycle-safe pointer traversal.
/// Storage grows only when the call reaches more nodes than previous calls.
///
/// # Safety
/// Shared FFI contract. `schema` is a complete compiler graph. `source` is null
/// or points to script storage of the graph's root type. `path` has `length` bytes.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_watch(
    ctx: *mut Context,
    source: *mut u8,
    elements: u64,
    indexed: u32,
    schema: *const u8,
    path: *const u8,
    length: u32,
    position: u32,
) -> *mut u8 {
    if source.is_null() {
        return std::ptr::null_mut();
    }
    let snapshot = unsafe { &mut *ctx }.boundary_watch.acquire() as *mut Snapshot;
    // The pool owns a stable box. No reference to Context survives this acquisition.
    let snapshot = unsafe { &mut *snapshot };
    snapshot.path = path;
    snapshot.length = length;
    snapshot.position = position;
    let root = unsafe { word(schema, 0) } as usize;
    let count = unsafe { word(schema, 4) };
    let stride = unsafe { word(schema, 8) } as usize;
    let mut cursor = unsafe { schema.add(12) };
    for _ in 0..count {
        snapshot.nodes.push(cursor);
        let members = unsafe { word(cursor, 0) };
        cursor = unsafe { cursor.add(8) };
        for _ in 0..members {
            cursor = unsafe { cursor.add(16 + word(cursor, 12) as usize) };
        }
    }
    for index in 0..elements as usize {
        snapshot.visit(Visit {
            source: unsafe { source.add(index * stride) },
            node: root,
            parent: None,
            member: std::ptr::null(),
            element: (indexed != 0).then_some(index),
        });
    }
    let mut visit = 0;
    while visit < snapshot.visits.len() {
        let source = snapshot.visits[visit].source;
        let mut member = snapshot.nodes[snapshot.visits[visit].node];
        let count = unsafe { word(member, 0) };
        member = unsafe { member.add(8) };
        for _ in 0..count {
            let tag = unsafe { word(member, 0) };
            let offset = unsafe { word(member, 4) } as usize;
            let extra = unsafe { word(member, 8) } as usize;
            let length = unsafe { word(member, 12) };
            let slot = unsafe { source.add(offset) };
            match tag {
                0 => snapshot.slots.push(Slot {
                    address: slot.cast(),
                    before: unsafe { address(slot, 0) },
                    member,
                    visit,
                }),
                1 | 2 => snapshot.visit(Visit {
                    source: if tag == 1 {
                        (unsafe { address(slot, 0) }) as *mut u8
                    } else {
                        slot
                    },
                    node: extra,
                    parent: Some(visit),
                    member,
                    element: None,
                }),
                3 => {
                    let array = unsafe { address(slot, 0) };
                    if !array.is_null() {
                        let ctx = unsafe { &*ctx };
                        let count = unsafe { ctx.array_len(array) } as usize;
                        let data = unsafe { ctx.array_data(array) } as *mut u8;
                        let stride = unsafe { word(snapshot.nodes[extra], 4) } as usize;
                        for index in 0..count {
                            snapshot.visit(Visit {
                                source: unsafe { data.add(index * stride) },
                                node: extra,
                                parent: Some(visit),
                                member,
                                element: Some(index),
                            });
                        }
                    }
                }
                _ => {}
            }
            member = unsafe { member.add(16 + length as usize) };
        }
        visit += 1;
    }
    (snapshot as *mut Snapshot).cast()
}

/// Compares captured slots; the call scope owns the snapshot storage.
///
/// # Safety
/// Shared FFI contract. `snapshot` is null or comes from a live `boundary_watch` call.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_watch_finish(ctx: *mut Context, snapshot: *mut u8) {
    if snapshot.is_null() {
        return;
    }
    let ctx = unsafe { &mut *ctx };
    if ctx.trapped() {
        return;
    }
    let snapshot = unsafe { &*snapshot.cast::<Snapshot>() };
    for slot in &snapshot.slots {
        let after = unsafe { slot.address.read_unaligned() };
        let nullable = unsafe { word(slot.member, 8) } != 0;
        if after != slot.before && (!after.is_null() || !nullable) {
            let path = unsafe { snapshot.error_path(slot) };
            fault(
                ctx,
                &path,
                "C replaced a pointer that the script cannot receive",
                snapshot.position,
            );
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_buffers_reuse_words_and_keep_nested_snapshots_separate() {
        let mut ctx = Context::new();
        let scope = ctx.boundary_scope_mark();
        let first = unsafe { subscript_rt_boundary_watch_buffer(&mut *ctx, 16, 7) };
        assert!(!first.is_null());
        unsafe { first.write(91) };
        let nested = ctx.boundary_scope_mark();
        let second = unsafe { subscript_rt_boundary_watch_buffer(&mut *ctx, 16, 7) };
        assert_ne!(first, second);
        ctx.boundary_scope_release(nested);
        assert_eq!(unsafe { first.read() }, 91);
        ctx.boundary_scope_release(scope);
        let scope = ctx.boundary_scope_mark();
        let reused = unsafe { subscript_rt_boundary_watch_buffer(&mut *ctx, 8, 7) };
        assert_eq!(first, reused);
        assert_eq!(unsafe { reused.read() }, 91);
        assert_eq!(ctx.boundary_watch.buffers[0].len(), 16);
        ctx.boundary_scope_release(scope);
        let failed = unsafe { subscript_rt_boundary_watch_buffer(&mut *ctx, u64::MAX, 29) };
        assert!(failed.is_null());
        assert_eq!(
            ctx.trap_record().expect("allocation trap").kind,
            crate::trap::TrapKind::AllocationFailure
        );
    }

    #[test]
    fn inline_error_materializes_only_the_supplied_indices() {
        let mut ctx = Context::new();
        let path = b"fill.out.groups[].items[].leaf";
        let indices = [2u64, 7];
        unsafe {
            subscript_rt_boundary_watch_error(
                &mut *ctx,
                path.as_ptr(),
                path.len() as u32,
                indices.as_ptr(),
                2,
                29,
            )
        };
        let trap = ctx.trap_record().expect("replacement trap");
        assert_eq!(trap.kind as u32, 37);
        assert_eq!(trap.pos_id, 29);
        assert_eq!(
            trap.message,
            "fill.out.groups[2].items[7].leaf: C replaced a pointer that the script cannot receive"
        );
    }

    #[test]
    fn unchanged_calls_reuse_snapshot_storage_and_nested_scopes() {
        let mut ctx = Context::new();
        let mut schema = Vec::new();
        for value in [0u32, 1, 8, 1, 8, 0, 0, 1, 4] {
            schema.extend(value.to_le_bytes());
        }
        schema.extend(b"next");
        let mut slot: *const u8 = std::ptr::null();
        let capture = |ctx: &mut Context| unsafe {
            subscript_rt_boundary_watch(
                ctx,
                (&mut slot as *mut *const u8).cast(),
                1,
                0,
                schema.as_ptr(),
                b"host.out".as_ptr(),
                8,
                12,
            )
        };
        let mut capture = capture;
        let mut storage = None;
        for _ in 0..1000 {
            let scope = ctx.boundary_scope_mark();
            let pointer = capture(&mut ctx);
            let snapshot = unsafe { &*pointer.cast::<Snapshot>() };
            let capacities = (
                snapshot.slots.capacity(),
                snapshot.nodes.capacity(),
                snapshot.visits.capacity(),
                snapshot.seen.capacity(),
            );
            if let Some((expected, address)) = storage {
                assert_eq!(capacities, expected);
                assert_eq!(pointer, address);
            }
            storage = Some((capacities, pointer));
            let nested = ctx.boundary_scope_mark();
            let second = capture(&mut ctx);
            assert_ne!(pointer, second);
            ctx.boundary_scope_release(nested);
            assert_eq!(ctx.boundary_watch.mark(), (1, 0));
            unsafe { subscript_rt_boundary_watch_finish(&mut *ctx, pointer) };
            ctx.boundary_scope_release(scope);
            assert_eq!(ctx.boundary_watch.mark(), (0, 0));
        }
        assert!(!ctx.trapped());
    }

    #[test]
    fn watches_compare_the_c_write_and_accept_null() {
        let mut ctx = Context::new();
        // One pointer slot, followed through a cycle to the same node.
        let mut schema = Vec::new();
        for value in [0u32, 1, 8, 2, 8] {
            schema.extend(value.to_le_bytes());
        }
        for tag in [0u32, 1] {
            for value in [tag, 0, if tag == 0 { 1 } else { 0 }, 4] {
                schema.extend(value.to_le_bytes());
            }
            schema.extend(b"next");
        }
        let mut slot: *const u8 = std::ptr::null();
        slot = (&slot as *const *const u8).cast();
        let path = b"host.Node";
        for replacement in [path.as_ptr(), std::ptr::null()] {
            let snapshot = unsafe {
                subscript_rt_boundary_watch(
                    &mut *ctx,
                    (&mut slot as *mut *const u8).cast(),
                    1,
                    0,
                    schema.as_ptr(),
                    path.as_ptr(),
                    path.len() as u32,
                    12,
                )
            };
            slot = replacement;
            unsafe { subscript_rt_boundary_watch_finish(&mut *ctx, snapshot) };
            if replacement.is_null() {
                assert!(!ctx.trapped());
            } else {
                let trap = ctx.trap_record().expect("replacement trap");
                assert_eq!(trap.kind as u32, 37);
                assert_eq!(
                    trap.message,
                    "host.Node.next: C replaced a pointer that the script cannot receive"
                );
                assert_eq!(trap.pos_id, 12);
                ctx.clear_trap();
            }
            slot = (&slot as *const *const u8).cast();
        }
    }
}
