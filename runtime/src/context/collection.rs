//! Automatic collection state of a Context (`compiler.md` §342).
//!
//! An allocation sets the pending flag; only a poll, the outermost exit,
//! or an explicit collection collects. An automatic collection records the
//! failure of an unobserved task instead of a trap.

use super::*;

/// The collection mode of a Context (`compiler.md` §342 rule 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[repr(i32)]
pub enum CollectionMode {
    /// No allocation sets the pending flag, and rules 14 and 15 do not run.
    Off = 0,
    /// A collection runs when live bytes exceed the threshold.
    Automatic = 1,
    /// Each poll and each outermost exit collects. This mode is for tests.
    EachPoll = 2,
}

impl CollectionMode {
    /// Returns the mode for its host value, or `None` for an unknown value.
    #[must_use]
    pub fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::Off),
            1 => Some(Self::Automatic),
            2 => Some(Self::EachPoll),
            _ => None,
        }
    }
}

/// The collection mode of a new Context (`compiler.md` §342 rule 17).
pub const DEFAULT_COLLECTION_MODE: CollectionMode = CollectionMode::Automatic;

/// The default minimum headroom in bytes (`compiler.md` §342 rule 6).
pub const DEFAULT_MIN_HEADROOM_BYTES: usize = 16 << 20;

/// The largest detection interval in outermost exits (`compiler.md` §342
/// rule 15).
pub const MAX_DETECTION_INTERVAL: u32 = 1024;

/// The mode and the minimum headroom. A worker Context takes the values of
/// its parent at the spawn (`compiler.md` §342 rule 18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollectionSettings {
    pub(crate) mode: CollectionMode,
    pub(crate) min_headroom: usize,
}

/// One failure of an unobserved task that an automatic collection
/// recorded (`compiler.md` §342 rule 12).
#[derive(Debug, Clone)]
struct RecordedFailure {
    task_id: u64,
    exception: crate::exception::PendingException,
}

/// The kind of a collection, for the counters that tests read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionKind {
    Explicit,
    Automatic,
    Detection,
}

/// The collection state of a Context. The pending flag is not here: it is
/// in the fixed prefix of [`Context`], because generated code reads it.
#[derive(Debug)]
pub(crate) struct CollectionState {
    settings: CollectionSettings,
    // The live bytes after the last collection, 0 for a new Context. The
    // threshold is computed from it (`compiler.md` §342 rules 6 and 17).
    base: usize,
    threshold: usize,
    // True while a collection runs. A released failed task is then
    // recorded, not trapped.
    recording: bool,
    recorded: Vec<RecordedFailure>,
    interval: u32,
    countdown: u32,
    automatic_collections: u64,
    detection_collections: u64,
    outermost_exits: u64,
    // The count of successful allocations since the Context was made.
    allocations: u64,
    // compiler.md §342 rule 14b: a failed unobserved task can exist.
    task_failure_possible: bool,
}

impl CollectionState {
    pub(crate) fn new() -> Self {
        let settings = CollectionSettings {
            mode: DEFAULT_COLLECTION_MODE,
            min_headroom: DEFAULT_MIN_HEADROOM_BYTES,
        };
        Self {
            settings,
            base: 0,
            threshold: threshold_for(0, settings.min_headroom),
            recording: false,
            recorded: Vec::new(),
            interval: 1,
            countdown: 1,
            automatic_collections: 0,
            detection_collections: 0,
            outermost_exits: 0,
            allocations: 0,
            task_failure_possible: false,
        }
    }

    /// The initial value of the pending flag for the default mode.
    pub(crate) fn initial_pending() -> u8 {
        u8::from(DEFAULT_COLLECTION_MODE == CollectionMode::EachPoll)
    }
}

/// `compiler.md` §342 rules 6 and 17: the base plus the larger of the base
/// and the minimum headroom. The base is the live bytes after the last
/// collection.
fn threshold_for(base: usize, min_headroom: usize) -> usize {
    base.saturating_add(base.max(min_headroom))
}

/// The collection work that an outermost exit leaves to its caller
/// (`compiler.md` §342 rule 16).
///
/// [`Context::exit_script_deferred`] returns it, and
/// [`Context::finish_exit_service`] performs it.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitService {
    serve: bool,
    detection: bool,
}

impl ExitService {
    const NONE: Self = Self {
        serve: false,
        detection: false,
    };

    /// True when a detection collection is due (`compiler.md` §342 rule 14).
    #[must_use]
    pub fn detection_due(&self) -> bool {
        self.detection
    }
}

impl Context {
    /// The byte offset of the one-byte pending flag in the Context. Generated
    /// code loads the flag at this offset (`compiler.md` §342 rule 9).
    #[must_use]
    pub const fn collection_pending_offset() -> usize {
        std::mem::offset_of!(Context, collection_pending)
    }

    /// True when the pending flag is set (`compiler.md` §342 rule 5).
    #[must_use]
    pub fn collection_pending(&self) -> bool {
        self.collection_pending != 0
    }

    /// The collection mode (`compiler.md` §342 rule 17).
    #[must_use]
    pub fn collection_mode(&self) -> CollectionMode {
        self.collection.settings.mode
    }

    /// The minimum headroom in bytes (`compiler.md` §342 rule 6).
    #[must_use]
    pub fn collection_min_headroom(&self) -> usize {
        self.collection.settings.min_headroom
    }

    /// The live-byte threshold above which an allocation sets the pending
    /// flag (`compiler.md` §342 rule 6).
    #[must_use]
    pub fn collection_threshold(&self) -> usize {
        self.collection.threshold
    }

    /// The number of automatic collections: polls and the outermost-exit
    /// service of `compiler.md` §342 rule 8.
    #[must_use]
    pub fn automatic_collection_count(&self) -> u64 {
        self.collection.automatic_collections
    }

    /// The number of detection collections (`compiler.md` §342 rule 15).
    #[must_use]
    pub fn detection_collection_count(&self) -> u64 {
        self.collection.detection_collections
    }

    /// The number of outermost exits (`compiler.md` §342 rule 16).
    #[must_use]
    pub fn outermost_exit_count(&self) -> u64 {
        self.collection.outermost_exits
    }

    /// The count of successful allocations since the Context was made.
    ///
    /// The interpreter compares it before and after an instruction that
    /// states "cannot allocate" (`compiler.md` §342 rule 10a).
    #[must_use]
    pub fn successful_allocations(&self) -> u64 {
        self.collection.allocations
    }

    /// The detection interval in outermost exits (`compiler.md` §342 rule 14).
    #[must_use]
    pub fn detection_interval(&self) -> u32 {
        self.collection.interval
    }

    /// The number of failures that automatic collections recorded and no
    /// report delivered yet (`compiler.md` §342 rule 12).
    #[must_use]
    pub fn recorded_failure_count(&self) -> usize {
        self.collection.recorded.len()
    }

    /// Sets the mode and the minimum headroom (`compiler.md` §342 rule 17).
    /// A `min_headroom_bytes` of 0 keeps the default. Returns false, with no
    /// change, while script depth is not zero.
    pub fn set_collection(&mut self, mode: CollectionMode, min_headroom_bytes: usize) -> bool {
        if self.script_depth != 0 {
            return false;
        }
        let min_headroom = if min_headroom_bytes == 0 {
            DEFAULT_MIN_HEADROOM_BYTES
        } else {
            min_headroom_bytes
        };
        self.apply_collection_settings(CollectionSettings { mode, min_headroom });
        true
    }

    pub(crate) fn collection_settings(&self) -> CollectionSettings {
        self.collection.settings
    }

    /// `compiler.md` §342 rule 17: the call runs no collection, so the
    /// threshold keeps its base. In mode 1 the flag is set if live bytes
    /// exceed the new threshold.
    pub(crate) fn apply_collection_settings(&mut self, settings: CollectionSettings) {
        self.collection.settings = settings;
        self.collection.threshold = threshold_for(self.collection.base, settings.min_headroom);
        self.collection_pending = match settings.mode {
            CollectionMode::Off => 0,
            CollectionMode::Automatic => {
                u8::from(self.live_bytes_counter > self.collection.threshold)
            }
            CollectionMode::EachPoll => 1,
        };
    }

    /// `compiler.md` §342 rule 5: a successful allocation sets the pending
    /// flag when live bytes exceed the threshold. It never collects.
    #[inline]
    pub(super) fn note_allocation(&mut self) {
        self.collection.allocations = self.collection.allocations.wrapping_add(1);
        if self.collection.settings.mode != CollectionMode::Off
            && self.live_bytes_counter > self.collection.threshold
        {
            self.collection_pending = 1;
        }
    }

    /// `compiler.md` §342 rules 6 and 7: after each collection the
    /// threshold follows the live set, and the flag clears. In mode 2 the
    /// flag stays set.
    fn after_collection(&mut self, kind: CollectionKind) {
        match kind {
            CollectionKind::Explicit => {}
            CollectionKind::Automatic => self.collection.automatic_collections += 1,
            CollectionKind::Detection => self.collection.detection_collections += 1,
        }
        self.collection.base = self.live_bytes_counter;
        self.collection.threshold =
            threshold_for(self.collection.base, self.collection.settings.min_headroom);
        self.collection_pending =
            u8::from(self.collection.settings.mode == CollectionMode::EachPoll);
    }

    /// Runs `collect` with the failure record active, then sets the
    /// threshold. `collect` performs one full collection of the owner's
    /// roots.
    fn run_collection<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        kind: CollectionKind,
        collect: impl FnOnce(&mut T) -> R,
    ) -> (R, bool) {
        let outer = std::mem::replace(&mut context(owner).collection.recording, true);
        let result = collect(owner);
        let ctx = context(owner);
        ctx.collection.recording = outer;
        ctx.after_collection(kind);
        (result, outer)
    }

    /// An explicit collection that the owner of the roots performs
    /// (`compiler.md` §342 rule 13). After `collect`, the Context reports
    /// one failure: the lowest task id of the recorded failures and of the
    /// failures that this collection found. It drops the other records.
    pub fn explicit_collection_with<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        collect: impl FnOnce(&mut T) -> R,
    ) -> R {
        let (result, outer) =
            Self::run_collection(owner, context, CollectionKind::Explicit, collect);
        if !outer {
            context(owner).report_recorded_failure();
        }
        result
    }

    /// An automatic collection that the owner of the roots performs
    /// (`compiler.md` §342 rules 7 and 12). It records the failure of an
    /// unobserved task and does not trap for it.
    pub fn automatic_collection_with<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        collect: impl FnOnce(&mut T) -> R,
    ) -> R {
        Self::run_collection(owner, context, CollectionKind::Automatic, collect).0
    }

    /// One automatic collection of the roots that the Context holds
    /// (`compiler.md` §342 rule 12).
    pub fn collect_automatic(&mut self) {
        Self::automatic_collection_with(self, |ctx| ctx, Context::collect_full);
    }

    /// The cold path of a poll (`compiler.md` §342 rule 7): collects when
    /// the pending flag is set and the Context has no trap.
    pub fn collect_poll(&mut self) {
        if self.collection_pending != 0 && !self.trapped() {
            self.collect_automatic();
        }
    }

    /// `compiler.md` §342 rule 12: records the failure of a task whose last
    /// owner a collection released. Returns false when no collection runs;
    /// the caller then traps as before.
    pub(super) fn record_unobserved_failure(
        &mut self,
        task_id: u64,
        exception: &crate::exception::PendingException,
    ) -> bool {
        if !self.collection.recording {
            return false;
        }
        self.collection.recorded.push(RecordedFailure {
            task_id,
            exception: exception.clone(),
        });
        self.note_task_failure();
        true
    }

    /// `compiler.md` §342 rule 14b: each site that stores a failure in a
    /// task calls this, so that the next outermost exit reads the tasks.
    pub(crate) fn note_task_failure(&mut self) {
        self.collection.task_failure_possible = true;
    }

    /// True when a failed unobserved task can exist (`compiler.md` §342
    /// rule 14b). The flag is clear only when no such task exists.
    #[must_use]
    pub fn task_failure_possible(&self) -> bool {
        self.collection.task_failure_possible
    }

    /// `compiler.md` §342 rules 14a and 14b: a failed unobserved task of
    /// the Context exists. The tasks are read only when the flag is set,
    /// and the flag clears when the read finds none and no record exists.
    fn own_failed_task(&mut self) -> bool {
        if !self.collection.task_failure_possible {
            return false;
        }
        let found = self.has_failed_unobserved_task();
        if !found && self.collection.recorded.is_empty() {
            self.collection.task_failure_possible = false;
        }
        found
    }

    /// `compiler.md` §342 rule 12 for a task that the caller owns outside
    /// the async frames of the Context. `object` is the Error object,
    /// `message` its report text, and `pos_id` the position of the
    /// `throw` in the caller's position table. Returns false when no
    /// collection runs; the caller then traps as before.
    pub fn record_task_failure(
        &mut self,
        task_id: u64,
        object: usize,
        message: String,
        pos_id: u32,
    ) -> bool {
        let exception = crate::exception::PendingException {
            object,
            message,
            pos_id,
        };
        self.record_unobserved_failure(task_id, &exception)
    }

    /// The Error objects of the recorded failures. They are collection
    /// roots (`compiler.md` §342 rule 12).
    pub(super) fn recorded_failure_objects(&self) -> impl Iterator<Item = usize> + '_ {
        self.collection
            .recorded
            .iter()
            .map(|failure| failure.exception.object)
    }

    /// `compiler.md` §342 rules 13 and 15: reports the recorded failure with
    /// the lowest task id as an `UncaughtException` trap at the throw
    /// position, and drops each record.
    fn report_recorded_failure(&mut self) {
        let recorded = std::mem::take(&mut self.collection.recorded);
        if let Some(failure) = recorded.into_iter().min_by_key(|failure| failure.task_id) {
            self.trap(
                TrapKind::UncaughtException,
                failure.exception.message,
                failure.exception.pos_id,
            );
        }
    }

    /// True when a task completed with an exception that no `await` raised,
    /// and the Context did not report it (`compiler.md` §342 rule 14a). Only
    /// an aggregate stays registered after its failure is reported: a
    /// reported task of another kind leaves the async frames.
    fn has_failed_unobserved_task(&self) -> bool {
        self.async_frames.values().any(|meta| {
            let reported = meta.kind.aggregate().is_some_and(|state| state.reported());
            !reported
                && matches!(
                    meta.completion.as_ref(),
                    Some(crate::exception::Completion::Exception(payload)) if !payload.observed
                )
        })
    }

    /// Marks return from script code and performs the collection work of
    /// the outermost exit (`compiler.md` §342 rules 8, 14, and 15).
    pub fn exit_script(&mut self) {
        let service = self.exit_script_deferred();
        Self::finish_exit_service(self, |ctx| ctx, service, Context::collect_full);
    }

    /// Marks return from script code, and returns the collection work that
    /// is due. The caller performs it with [`Context::finish_exit_service`].
    /// A caller that owns roots the Context does not hold uses this pair
    /// (`compiler.md` §342 rule 16).
    pub fn exit_script_deferred(&mut self) -> ExitService {
        self.exit_script_with_tasks(|| false)
    }

    /// [`Context::exit_script_deferred`] for a caller that owns tasks
    /// outside the async frames of the Context. `owner_failed_task` answers
    /// if one of them completed with an exception that no `await` raised
    /// (`compiler.md` §342 rule 14a). The Context calls it at most once,
    /// at an outermost exit with no recorded failure.
    pub fn exit_script_with_tasks(
        &mut self,
        owner_failed_task: impl FnOnce() -> bool,
    ) -> ExitService {
        self.script_depth = self.script_depth.saturating_sub(1);
        if self.script_depth != 0 {
            return ExitService::NONE;
        }
        // compiler.md §115.4 item 1: an exception that leaves the
        // outermost script frame of a host entry becomes a trap.
        self.settle_uncaught_exception();
        self.collection.outermost_exits += 1;
        // compiler.md §342 rule 14b: a debug build compares the clear flag
        // with the tasks at each outermost exit.
        debug_assert!(
            self.collection.task_failure_possible || !self.has_failed_unobserved_task(),
            "compiler.md §342 rule 14b: the flag is clear and a failed unobserved task exists"
        );
        if self.collection.settings.mode == CollectionMode::Off || self.trapped() {
            return ExitService::NONE;
        }
        // compiler.md §342 rules 14 and 14b.
        if self.collection.recorded.is_empty() && !self.own_failed_task() && !owner_failed_task() {
            self.collection.interval = 1;
            self.collection.countdown = 1;
            return ExitService {
                serve: true,
                detection: false,
            };
        }
        self.collection.countdown = self.collection.countdown.saturating_sub(1);
        ExitService {
            serve: true,
            detection: self.collection.countdown == 0,
        }
    }

    /// Performs the collection work of an outermost exit
    /// (`compiler.md` §342 rules 8 and 15). `collect` performs one full
    /// collection of the owner's roots; the Context records the failures
    /// that it finds.
    pub fn finish_exit_service<T>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        service: ExitService,
        mut collect: impl FnMut(&mut T),
    ) {
        if !service.serve {
            return;
        }
        if service.detection {
            Self::run_collection(owner, context, CollectionKind::Detection, &mut collect);
            let ctx = context(owner);
            if ctx.collection.recorded.is_empty() {
                let interval = ctx
                    .collection
                    .interval
                    .saturating_mul(2)
                    .min(MAX_DETECTION_INTERVAL);
                ctx.collection.interval = interval;
                ctx.collection.countdown = interval;
            } else {
                ctx.report_recorded_failure();
            }
        }
        let ctx = context(owner);
        if !ctx.trapped() && ctx.collection_pending != 0 {
            Self::run_collection(owner, context, CollectionKind::Automatic, &mut collect);
        }
    }
}

#[cfg(test)]
mod successful_allocation_tests {
    use super::*;

    #[test]
    fn successful_allocations_rise_with_each_successful_allocation_only() {
        let mut context = Context::new();
        assert_eq!(context.successful_allocations(), 0);
        let first = context.alloc(16, 0, 0);
        assert!(!first.is_null());
        let _ = context.alloc_str(b"text", 0);
        assert_eq!(context.successful_allocations(), 2);
        context.delete(first as usize, 0);
        assert_eq!(context.successful_allocations(), 2);
        context.fail_alloc_after(1);
        assert!(context.alloc(16, 0, 0).is_null());
        assert_eq!(context.successful_allocations(), 2);
    }
}
