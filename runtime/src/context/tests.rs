use super::*;

#[repr(C)]
struct TestCountedAsyncFrame {
    state: i32,
    count: u32,
    resume: AsyncResume,
}

unsafe extern "C" fn counted_test_resume(_ctx: *mut Context, _frame: *mut u8, _out: *mut u8) -> u8 {
    1
}

#[repr(C)]
struct TestSchedulerFrame {
    state: i32,
    count: u32,
    resume: AsyncResume,
    id: u8,
    polls: u8,
}

/// Counts every scheduler resume of `teardown_park_resume`, so a test
/// can observe that Context teardown runs no continuation.
static TEARDOWN_RESUMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn frame_label(id: u8) -> &'static [u8] {
    match id {
        1 => b"one".as_slice(),
        2 => b"two".as_slice(),
        _ => b"three".as_slice(),
    }
}

/// Allocates and registers one Context-owned scheduler frame with the
/// emitted coroutine header (§70.2), which the scheduler reads.
fn spawn_test_frame(ctx: &mut Context, id: u8, resume: AsyncResume) -> *mut u8 {
    let frame = ctx.alloc(
        std::mem::size_of::<TestSchedulerFrame>(),
        CLASS_GENERATOR,
        0,
    );
    // SAFETY: the allocation has exactly the test-frame payload.
    unsafe {
        frame
            .cast::<TestSchedulerFrame>()
            .write(TestSchedulerFrame {
                state: 0,
                count: 0,
                resume,
                id,
                polls: 0,
            });
        ctx.async_register(frame, 0);
    }
    frame
}

/// Parks twice and completes on its third resume, so one frame spans
/// the kick and two checkpoints, as two `Context.suspend()` awaits do.
unsafe extern "C" fn parking_test_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: the tests pass matching live `TestSchedulerFrame` values.
    let context = unsafe { &mut *ctx };
    let record = unsafe { &mut *frame.cast::<TestSchedulerFrame>() };
    context.print_line(frame_label(record.id));
    record.polls += 1;
    if record.polls == 3 {
        return 1;
    }
    // SAFETY: the frame is registered in this Context.
    unsafe { context.async_park(frame, 0) };
    0
}

/// Parks forever and collects on every resume after the first.
unsafe extern "C" fn collecting_park_resume(
    ctx: *mut Context,
    frame: *mut u8,
    _out: *mut u8,
) -> u8 {
    // SAFETY: the tests pass matching live `TestSchedulerFrame` values.
    let context = unsafe { &mut *ctx };
    let record = unsafe { &mut *frame.cast::<TestSchedulerFrame>() };
    context.print_line(frame_label(record.id));
    record.polls += 1;
    if record.polls > 1 {
        context.collect();
    }
    // SAFETY: the frame is registered in this Context.
    unsafe { context.async_park(frame, 0) };
    0
}

/// Parks forever and counts its resumes in `TEARDOWN_RESUMES`.
unsafe extern "C" fn teardown_park_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    TEARDOWN_RESUMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // SAFETY: the test passes a matching live `TestSchedulerFrame`.
    let context = unsafe { &mut *ctx };
    // SAFETY: the frame is registered in this Context.
    unsafe { context.async_park(frame, 0) };
    0
}

impl Context {
    /// Enumerable allocation count. Dev tier: total map length (live
    /// + retained-dead), distinguishing retain-and-poison (entry
    /// kept) from release (entry gone). Ship tier (§8.1b): there is
    /// no per-allocation table; the enumerable set is the live
    /// blocks plus large records, i.e. `live_count` — a released
    /// block leaves nothing behind.
    fn allocation_count(&self) -> usize {
        if self.uses_ship_arena() {
            self.live_count()
        } else {
            self.allocations.len() + self.retained_allocations.len()
        }
    }

    /// Number of arena chunks currently owned (ship tier).
    fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Shared handle to the arena resource balance, observable after
    /// the Context is dropped.
    fn test_stats(&self) -> std::sync::Arc<ArenaStats> {
        std::sync::Arc::clone(&self.stats)
    }

    /// Moves the live-byte counter away from the live set, so one
    /// test proves that the collection assertion of §113.2 rule 4
    /// reports a drifted counter.
    fn test_offset_live_bytes_counter(&mut self, delta: usize) {
        self.live_bytes_counter = self.live_bytes_counter.saturating_add(delta);
    }
}

/// Compares the maintained byte counter against the walk over the
/// live set (§113.2 rule 4).
fn assert_the_counter_matches_the_walk(ctx: &Context, mode: &str) {
    assert_eq!(
        ctx.live_bytes(),
        ctx.live_bytes_by_walk(),
        "{mode}: live bytes"
    );
}

const MARK_TRACE_TEST_CHILD: &str = "SUBSCRIPT_MARK_TRACE_TEST_CHILD";

fn assert_popped_element_is_unreachable(mut ctx: Box<Context>, tier: &str) {
    let x = ctx.alloc(16, 1, 0);
    let h = ctx.array_new(std::mem::size_of::<usize>(), 0);
    let mut root = h as usize;
    ctx.root_add((&mut root as *mut usize) as usize, 1);
    let value = x as usize;
    // SAFETY: `h` is a live word array, and `value` is readable.
    unsafe { ctx.array_push(h, (&raw const value).cast(), 0) };
    let mut out = 0usize;
    // SAFETY: `h` is a nonempty live word array, and `out` is writable.
    unsafe { ctx.array_pop(h, (&raw mut out).cast(), 0) };

    ctx.collect();

    assert!(!ctx.is_live(x as usize), "{tier}: popped element");
}

fn assert_array_tail_violation_is_reported(mut ctx: Box<Context>, tier: &str) {
    let h = ctx.array_with_capacity(4, std::mem::size_of::<usize>(), 0);
    let clean = ctx.array_with_capacity(4, std::mem::size_of::<usize>(), 0);
    let clean_value = 5usize;
    // SAFETY: `clean` is a live word array, and `clean_value` is readable.
    unsafe { ctx.array_push(clean, (&raw const clean_value).cast(), 0) };
    let mut roots = [h as usize, clean as usize];
    ctx.root_add(roots.as_mut_ptr() as usize, roots.len());
    let value = 7usize;
    // SAFETY: `h` is a live word array, and `value` is readable.
    unsafe { ctx.array_push(h, (&raw const value).cast(), 0) };
    let len = 1usize;
    let offset = len * std::mem::size_of::<usize>();
    // SAFETY: capacity four provides a writable word at `offset`.
    unsafe {
        ctx.array_data(h)
            .cast_mut()
            .add(offset)
            .cast::<usize>()
            .write(11);
    }

    assert_eq!(
        ctx.array_tail_violations(),
        vec![ArrayTailViolation {
            handle: h as usize,
            len: len as u64,
            offset,
        }],
        "{tier}"
    );

    let stale = 11usize;
    let mut popped = 0usize;
    // SAFETY: `h` is a live word array. The push makes the stale slot
    // live, and the pop copies it into writable storage before clearing it.
    unsafe {
        ctx.array_push(h, (&raw const stale).cast(), 0);
        ctx.array_pop(h, (&raw mut popped).cast(), 0);
    }
    assert_eq!(popped, stale, "{tier}");
    assert!(ctx.array_tail_violations().is_empty(), "{tier}");

    for value in [22usize, 33] {
        // SAFETY: `h` is live, and each `value` is readable.
        unsafe { ctx.array_push(h, (&raw const value).cast(), 0) };
    }
    // SAFETY: `h` has length three, so truncation to one is valid.
    unsafe { ctx.array_truncate(h, 1, 0) };
    // SAFETY: capacity four provides readable word slots one and two.
    unsafe {
        let data = ctx.array_data(h).cast::<usize>();
        assert_eq!(data.add(1).read(), 0, "{tier}: slot 1");
        assert_eq!(data.add(2).read(), 0, "{tier}: slot 2");
    }
    assert!(ctx.array_tail_violations().is_empty(), "{tier}");
}

fn assert_missing_array_data_is_reported(mut ctx: Box<Context>, tier: &str) {
    let h = ctx.array_with_capacity(4, std::mem::size_of::<usize>(), 0);
    // SAFETY: `h` is a live array header with allocated data.
    let data = unsafe { ctx.array_data(h) } as usize;
    ctx.delete(data, 0);

    assert_eq!(
        ctx.array_tail_violations(),
        vec![ArrayTailViolation {
            handle: h as usize,
            len: 0,
            offset: 0,
        }],
        "{tier}"
    );
}

#[cfg(debug_assertions)]
fn assert_collect_traps_on_array_tail_violation(mut ctx: Box<Context>, tier: &str) {
    let h = ctx.array_with_capacity(4, std::mem::size_of::<usize>(), 0);
    let value = 7usize;
    // SAFETY: `h` is a live word array, and `value` is readable.
    unsafe { ctx.array_push(h, (&raw const value).cast(), 0) };
    let offset = std::mem::size_of::<usize>();
    // SAFETY: capacity four provides a writable word at `offset`.
    unsafe {
        ctx.array_data(h)
            .cast_mut()
            .add(offset)
            .cast::<usize>()
            .write(13);
    }

    ctx.collect();

    let trap = ctx.trap_record().expect("array-tail trap");
    assert_eq!(trap.kind, TrapKind::Internal, "{tier}");
    assert_eq!(trap.pos_id, 0, "{tier}");
    assert!(
        trap.message.contains(&format!("offset={offset}")),
        "{tier}: {}",
        trap.message
    );
    assert!(ctx.is_live(h as usize), "{tier}: collection must not run");
}

#[path = "tests_0.rs"]
mod tests_0;

#[path = "tests_1.rs"]
mod tests_1;
