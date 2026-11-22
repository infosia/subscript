//! The drain of the scheduler serves the collection flag between two jobs
//! (`compiler.md` §351 rule 12). Each test runs in the dev allocator and in
//! the ship arena.

use std::cell::Cell;

use super::*;

/// The jobs of one drain.
const JOBS: usize = 256;

/// The garbage objects of each job, and the payload bytes of each.
const GARBAGE: usize = 4;
const GARBAGE_BYTES: usize = 64;

/// The minimum headroom of the tests.
const HEADROOM: usize = 256;

/// The payload of a test frame, with the coroutine header that the
/// scheduler reads (`compiler.md` §70.2).
#[repr(C)]
struct DrainFrame {
    state: i32,
    count: u32,
    resume: AsyncResume,
    resumes: u32,
}

thread_local! {
    /// The most live bytes that a job saw when it started.
    static MOST_LIVE: Cell<usize> = const { Cell::new(0) };
    /// The automatic collection count that the last job saw.
    static LAST_COLLECTIONS: Cell<u64> = const { Cell::new(0) };
    /// The garbage allocations that returned null.
    static FAILED_ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

/// Parks on the kick. On the resume in the drain, it samples the live
/// bytes, allocates `GARBAGE` unreachable objects, and completes.
unsafe extern "C" fn garbage_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: the scheduler passes the Context that runs this job, and no
    // other reference to it is live during the resume.
    let context = unsafe { &mut *ctx };
    // SAFETY: the tests register only `DrainFrame` payloads with this
    // resume function, and the frame stays live while its job runs.
    let record = unsafe { &mut *frame.cast::<DrainFrame>() };
    record.resumes += 1;
    if record.resumes == 1 {
        // SAFETY: the frame is registered in this Context.
        unsafe { context.async_park(frame, 0) };
        return 0;
    }
    MOST_LIVE.with(|most| most.set(most.get().max(context.live_bytes())));
    LAST_COLLECTIONS.with(|last| last.set(context.automatic_collection_count()));
    for _ in 0..GARBAGE {
        if context.alloc(GARBAGE_BYTES, 0, 0).is_null() {
            FAILED_ALLOCATIONS.with(|failed| failed.set(failed.get() + 1));
        }
    }
    1
}

/// Registers and kicks `JOBS` frames, so one step drains them as `JOBS`
/// ready jobs. Returns the live bytes after the kicks.
fn park_jobs(ctx: &mut Context) -> usize {
    for _ in 0..JOBS {
        let frame = ctx.alloc(std::mem::size_of::<DrainFrame>(), CLASS_GENERATOR, 0);
        // SAFETY: the allocation has exactly the test-frame payload, and
        // `garbage_resume` reads it as that payload.
        unsafe {
            frame.cast::<DrainFrame>().write(DrainFrame {
                state: 0,
                count: 0,
                resume: garbage_resume,
                resumes: 0,
            });
            ctx.async_register(frame, 0);
            ctx.async_kick(frame, garbage_resume);
        }
    }
    ctx.collect();
    ctx.live_bytes()
}

/// Mode 1 with a small headroom: one drain of `JOBS` jobs that each leave
/// garbage keeps the live bytes under a bound, and collects during the
/// drain. The bound: after a collection the limit is the larger of the live
/// bytes and the headroom (`compiler.md` §346 rule 2), and one job can
/// allocate before the next poll. Without the poll, the live bytes reach
/// `JOBS * GARBAGE * GARBAGE_BYTES`.
#[test]
fn a_drain_collects_between_two_jobs() {
    for mut ctx in [Context::new(), Context::new_releasing()] {
        assert!(ctx.set_collection(CollectionMode::Automatic, HEADROOM));
        let frames = park_jobs(&mut ctx);
        assert_eq!(ctx.async_parked_len(), JOBS);
        MOST_LIVE.with(|most| most.set(0));
        FAILED_ALLOCATIONS.with(|failed| failed.set(0));
        let before = ctx.automatic_collection_count();
        // SAFETY: each queued frame is a live `DrainFrame` of this Context.
        let pending = unsafe { ctx.async_step() };
        assert_eq!(pending, 0);
        assert_eq!(FAILED_ALLOCATIONS.with(Cell::get), 0);
        assert!(!ctx.trapped());
        let during = LAST_COLLECTIONS.with(Cell::get) - before;
        let most = MOST_LIVE.with(Cell::get);
        // The ship arena counts the size-class capacity: a factor of two
        // covers it.
        let bound = 2 * (2 * frames.max(HEADROOM) + GARBAGE * GARBAGE_BYTES);
        assert!(during > 0, "no collection during the drain");
        assert!(
            most <= bound,
            "live bytes {most} over the bound {bound} (frames {frames})"
        );
    }
}

/// Mode 0: the drain runs no collection between two jobs, and the live
/// bytes rise with the jobs. This is the firing control of the bound of
/// [`a_drain_collects_between_two_jobs`].
#[test]
fn a_drain_with_the_mode_off_does_not_collect() {
    for mut ctx in [Context::new(), Context::new_releasing()] {
        assert!(ctx.set_collection(CollectionMode::Off, HEADROOM));
        let frames = park_jobs(&mut ctx);
        MOST_LIVE.with(|most| most.set(0));
        FAILED_ALLOCATIONS.with(|failed| failed.set(0));
        let before = ctx.automatic_collection_count();
        // SAFETY: each queued frame is a live `DrainFrame` of this Context.
        unsafe { ctx.async_step() };
        assert_eq!(FAILED_ALLOCATIONS.with(Cell::get), 0);
        assert_eq!(LAST_COLLECTIONS.with(Cell::get), before);
        let most = MOST_LIVE.with(Cell::get);
        let bound = 2 * (2 * frames.max(HEADROOM) + GARBAGE * GARBAGE_BYTES);
        assert!(most > bound, "live bytes {most} under the bound {bound}");
    }
}

/// A budget of zero runs no job and no poll, and a budget of one runs one
/// job and no poll: a poll is between two jobs only. Mode 2 makes each
/// poll collect, so the count shows each poll.
#[test]
fn a_drain_polls_between_two_jobs_only() {
    for mut ctx in [Context::new(), Context::new_releasing()] {
        assert!(ctx.set_collection(CollectionMode::EachPoll, HEADROOM));
        park_jobs(&mut ctx);
        FAILED_ALLOCATIONS.with(|failed| failed.set(0));
        let before = ctx.automatic_collection_count();
        // SAFETY: each queued frame is a live `DrainFrame` of this Context.
        let report = unsafe { ctx.async_step_budget(0) };
        assert_eq!(report.dispatched, 0);
        assert_eq!(ctx.automatic_collection_count(), before);
        // SAFETY: as above.
        let report = unsafe { ctx.async_step_budget(1) };
        assert_eq!(report.dispatched, 1);
        // One collection: the outermost exit of the step (§342 rule 8).
        assert_eq!(ctx.automatic_collection_count(), before + 1);
        // SAFETY: as above.
        let report = unsafe { ctx.async_step_budget(3) };
        assert_eq!(report.dispatched, 3);
        // Two polls between three jobs, and the outermost exit.
        assert_eq!(ctx.automatic_collection_count(), before + 4);
        // The poll is not an exit: the two steps that ran jobs are the exits.
        assert_eq!(ctx.outermost_exit_count(), 2);
        assert_eq!(FAILED_ALLOCATIONS.with(Cell::get), 0);
    }
}
