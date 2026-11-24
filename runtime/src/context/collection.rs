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
    /// A collection runs when the bytes requested since the last
    /// collection exceed the limit (`compiler.md` §346 rule 2).
    Automatic = 1,
    /// Each poll and each outermost exit collects. This mode is for tests.
    EachPoll = 2,
    /// The step stress mode, for tests (`compiler.md` §356 rule 19). Each
    /// poll and each outermost exit runs one pause of a cycle with the
    /// budget that the host set, and a new cycle starts when one ends. Each
    /// end runs the check of the mark and stops the process with a message
    /// if the check fails. A Context with no step form, or a budget of 0,
    /// collects at each poll as in mode 2.
    StepStress = 3,
}

impl CollectionMode {
    /// Returns the mode for its host value, or `None` for an unknown value.
    #[must_use]
    pub fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::Off),
            1 => Some(Self::Automatic),
            2 => Some(Self::EachPoll),
            3 => Some(Self::StepStress),
            _ => None,
        }
    }
}

/// The collection mode of a new Context (`compiler.md` §342 rule 17).
pub const DEFAULT_COLLECTION_MODE: CollectionMode = CollectionMode::Automatic;

/// The default minimum headroom in bytes (`compiler.md` §346 rule 2).
pub const DEFAULT_MIN_HEADROOM_BYTES: usize = 16 << 20;

/// The step budget in words of a new Context (`compiler.md` §356 rule
/// 16). A budget of 0 makes each collection one pause.
pub const DEFAULT_COLLECTION_STEP_WORDS: usize = 50_000;

// The smallest prune limit of the observed race inputs (§350 rule 5).
const OBSERVED_INPUTS_MIN_LIMIT: usize = 16;

/// The largest detection interval in outermost exits (`compiler.md` §342
/// rule 15).
pub const MAX_DETECTION_INTERVAL: u32 = 1024;

/// The mode, the minimum headroom, and the step budget. A worker Context
/// takes the values of its parent at the spawn (`compiler.md` §342 rule 18
/// and §356 rule 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollectionSettings {
    pub(crate) mode: CollectionMode,
    pub(crate) min_headroom: usize,
    // compiler.md §356 rule 16: the budget of one mark step in words.
    pub(crate) step_budget: usize,
}

/// One failure of an unobserved task that an automatic collection
/// recorded (`compiler.md` §342 rule 12).
#[derive(Debug, Clone)]
struct RecordedFailure {
    task_id: u64,
    exception: crate::exception::PendingException,
}

/// The kind of a collection, for the counters that tests read and for the
/// class of `compiler.md` §353 rule 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CollectionKind {
    Explicit,
    Automatic,
    Detection,
}

/// The collection state of a Context. The pending flag is not here: it is
/// in the fixed prefix of [`Context`], because generated code reads it.
#[derive(Debug)]
pub(crate) struct CollectionState {
    pub(super) settings: CollectionSettings,
    // The live bytes after the last collection, 0 for a new Context. The
    // limit is computed from it (`compiler.md` §346 rules 2 and 3).
    base: usize,
    pub(super) limit: usize,
    // The requested bytes above which an allocation sets the pending flag.
    // It is the limit, except in the step form of `compiler.md` §356: the
    // start of a cycle before the limit (rule 7), and each part of the
    // headroom during a cycle (rule 6).
    pub(super) pending_at: usize,
    // The bytes that allocations requested since the last collection,
    // less the explicit releases (`compiler.md` §346 rule 1).
    pub(super) requested: usize,
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
    // compiler.md §350 rule 5: the inputs of a race that had no completion
    // when the race took them, as (frame, task id). A failure that one of
    // them stores is observed. The list is pruned when it reaches the limit.
    observed_inputs: Vec<(usize, u64)>,
    observed_inputs_limit: usize,
    // compiler.md §353 rule 1: the statistics of the collections.
    pub(super) stats: super::collection_stats::StatsState,
}

impl CollectionState {
    pub(crate) fn new() -> Self {
        let settings = CollectionSettings {
            mode: DEFAULT_COLLECTION_MODE,
            min_headroom: DEFAULT_MIN_HEADROOM_BYTES,
            step_budget: DEFAULT_COLLECTION_STEP_WORDS,
        };
        Self {
            settings,
            base: 0,
            limit: limit_for(0, settings.min_headroom),
            // A new Context has no arena yet, so its threshold is the limit;
            // the first change of the settings or the first collection sets
            // the threshold of the step form.
            pending_at: limit_for(0, settings.min_headroom),
            requested: 0,
            recording: false,
            recorded: Vec::new(),
            interval: 1,
            countdown: 1,
            automatic_collections: 0,
            detection_collections: 0,
            outermost_exits: 0,
            allocations: 0,
            task_failure_possible: false,
            observed_inputs: Vec::new(),
            observed_inputs_limit: OBSERVED_INPUTS_MIN_LIMIT,
            stats: super::collection_stats::StatsState::default(),
        }
    }

    /// The initial value of the pending flag for the default mode.
    pub(crate) fn initial_pending() -> u8 {
        u8::from(matches!(
            DEFAULT_COLLECTION_MODE,
            CollectionMode::EachPoll | CollectionMode::StepStress
        ))
    }
}

/// `compiler.md` §346 rules 2 and 3: the larger of the base and the minimum
/// headroom. The base is the live bytes after the last collection.
fn limit_for(base: usize, min_headroom: usize) -> usize {
    base.max(min_headroom)
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

    /// The minimum headroom in bytes (`compiler.md` §346 rule 2).
    #[must_use]
    pub fn collection_min_headroom(&self) -> usize {
        self.collection.settings.min_headroom
    }

    /// The limit of the requested bytes: an allocation sets the pending
    /// flag when the requested bytes exceed it. It is the larger of the
    /// live bytes after the last collection and the minimum headroom
    /// (`compiler.md` §346 rule 2).
    #[must_use]
    pub fn collection_limit(&self) -> usize {
        self.collection.limit
    }

    /// The bytes that allocations requested since the last collection,
    /// less the sizes of the explicit releases, not under zero
    /// (`compiler.md` §346 rule 1).
    #[must_use]
    pub fn requested_bytes(&self) -> usize {
        self.collection.requested
    }

    /// The number of automatic collections: polls and the outermost-exit
    /// service of `compiler.md` §342 rule 8. It does not count a detection
    /// collection (§342 rule 14); the record of §353 counts a detection
    /// collection as automatic.
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

    /// Sets the mode and the minimum headroom (`compiler.md` §342 rule 17 and
    /// §346 rule 3).
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
        self.apply_collection_settings(CollectionSettings {
            mode,
            min_headroom,
            step_budget: self.collection.settings.step_budget,
        });
        true
    }

    pub(crate) fn collection_settings(&self) -> CollectionSettings {
        self.collection.settings
    }

    /// `compiler.md` §346 rule 3: the call runs no collection, so the limit
    /// keeps its base and the requested bytes stay. In mode 1 the flag is
    /// set if the requested bytes exceed the new limit.
    ///
    /// `compiler.md` §356 rules 6 and 16: during a cycle in mode 1 the flag
    /// is set, so that the next poll serves the cycle with the new values.
    pub(crate) fn apply_collection_settings(&mut self, settings: CollectionSettings) {
        self.collection.settings = settings;
        self.collection.limit = limit_for(self.collection.base, settings.min_headroom);
        if !self.mark_cycle_active() {
            self.collection.pending_at = self.cycle_start_threshold();
        }
        self.collection_pending = match settings.mode {
            CollectionMode::Off => 0,
            CollectionMode::Automatic => u8::from(
                self.mark_cycle_active() || self.collection.requested > self.collection.pending_at,
            ),
            CollectionMode::EachPoll | CollectionMode::StepStress => 1,
        };
    }

    /// `compiler.md` §346 rules 1 and 2: a successful allocation adds the
    /// `requested` payload size to the requested bytes, and sets the
    /// pending flag when they exceed the limit. It never collects
    /// (`compiler.md` §342 rule 5).
    #[inline]
    pub(super) fn note_allocation(&mut self, requested: usize) {
        self.collection.allocations = self.collection.allocations.wrapping_add(1);
        self.collection.requested = self.collection.requested.saturating_add(requested);
        if self.collection.settings.mode != CollectionMode::Off
            && self.collection.requested > self.collection.pending_at
        {
            self.collection_pending = 1;
        }
    }

    /// `compiler.md` §346 rules 1 and 1a: a release outside a collection
    /// subtracts the `request` that the allocation of the block added, as
    /// the block records it. The count does not go under zero.
    #[inline]
    pub(super) fn note_release(&mut self, request: usize) {
        self.collection.requested = self.collection.requested.saturating_sub(request);
    }

    /// `compiler.md` §346 rules 1 and 2, and §342 rule 7: after each
    /// collection the requested bytes are zero, the limit follows the live
    /// set, and the flag clears. In mode 2 and mode 3 the flag stays set.
    fn after_collection(&mut self, kind: CollectionKind) {
        match kind {
            CollectionKind::Explicit => {}
            CollectionKind::Automatic => self.collection.automatic_collections += 1,
            CollectionKind::Detection => self.collection.detection_collections += 1,
        }
        self.collection.base = self.live_bytes_counter;
        self.collection.limit =
            limit_for(self.collection.base, self.collection.settings.min_headroom);
        self.collection.requested = 0;
        self.collection.pending_at = self.cycle_start_threshold();
        self.collection_pending = u8::from(matches!(
            self.collection.settings.mode,
            CollectionMode::EachPoll | CollectionMode::StepStress
        ));
    }

    /// Runs `collect` with the failure record active, then sets the
    /// limit. `collect` performs one full collection of the owner's
    /// roots.
    fn run_collection<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        kind: CollectionKind,
        collect: impl FnOnce(&mut T) -> R,
    ) -> (R, bool) {
        /// Restores the outer value of the failure record when the
        /// collection ends, also on an unwind (`compiler.md` §342 rule 12).
        struct Restore<'a, T> {
            owner: &'a mut T,
            context: fn(&mut T) -> &mut Context,
            outer: bool,
        }
        impl<T> Drop for Restore<'_, T> {
            fn drop(&mut self) {
                (self.context)(self.owner).collection.recording = self.outer;
            }
        }
        let outer = std::mem::replace(&mut context(owner).collection.recording, true);
        // compiler.md §353 rule 4: the outermost collection is timed. A
        // collection inside it is part of its work. A collection that
        // unwinds does not end, so it is not recorded (rule 1).
        let started = (!outer).then(|| context(owner).collection.stats.start());
        let guard = Restore {
            owner,
            context,
            outer,
        };
        let result = collect(&mut *guard.owner);
        drop(guard);
        let ctx = context(owner);
        if let Some(started) = started {
            let automatic = kind != CollectionKind::Explicit;
            let live_bytes = ctx.live_bytes_counter;
            ctx.collection.stats.finish(started, automatic, live_bytes);
        }
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
        self.run_own_collection(CollectionKind::Automatic);
    }

    /// One explicit collection of the roots that the Context holds, with the
    /// report of `compiler.md` §342 rule 13.
    pub(super) fn collect_explicit_own(&mut self) {
        if !self.run_own_collection(CollectionKind::Explicit) {
            self.report_recorded_failure();
        }
    }

    /// One collection of the class `kind` of the roots that the Context
    /// holds. Returns true when it runs inside another collection.
    ///
    /// `compiler.md` §356: an automatic collection of the step form runs one
    /// pause of a cycle (rule 6). Another collection ends a cycle in
    /// progress first (rule 8), and then runs one collection with no step.
    /// A poll with a budget of 0 only ends the cycle (rule 16).
    pub(super) fn run_own_collection(&mut self, kind: CollectionKind) -> bool {
        if self.collection.recording {
            return self.run_whole_collection(kind);
        }
        let mode = self.collection.settings.mode;
        if kind == CollectionKind::Automatic
            && self.has_poll_step_form()
            && matches!(mode, CollectionMode::Automatic | CollectionMode::StepStress)
        {
            let budget = self.collection.settings.step_budget;
            self.run_cycle_pause(budget);
            return false;
        }
        if self.mark_cycle_active() {
            self.run_whole_collection(CollectionKind::Automatic);
            if kind == CollectionKind::Automatic && mode != CollectionMode::EachPoll {
                return false;
            }
        }
        self.run_whole_collection(kind)
    }

    /// One collection of the class `kind` in one pause. Returns true when
    /// it runs inside another collection. If a cycle is in progress, it is
    /// the end of that cycle (`compiler.md` §356 rule 5).
    ///
    /// `compiler.md` §353 rule 4: a debug build checks the array tails
    /// before the time starts, and the live bytes after the record is
    /// finished. A violation of the tails traps, and the collection then
    /// does no work.
    pub(super) fn run_whole_collection(&mut self, kind: CollectionKind) -> bool {
        #[cfg(debug_assertions)]
        {
            let violations = self.array_tail_violations();
            if !violations.is_empty() {
                let details = violations
                    .iter()
                    .map(|violation| {
                        format!(
                            "handle={}, len={}, offset={}",
                            violation.handle, violation.len, violation.offset
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                self.trap(
                    TrapKind::Internal,
                    format!("array tail violations: [{details}]"),
                    0,
                );
                return Self::run_collection(self, |ctx| ctx, kind, |_| ()).1;
            }
        }
        let (_, outer) = Self::run_collection(self, |ctx| ctx, kind, Context::collect_full);
        // §113.2 rule 4: the maintained counter and the walk over the live
        // set are two independent derivations of one quantity.
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.live_bytes_by_walk(), self.live_bytes());
        outer
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
        // compiler.md §350 rule 5: each site that stores a failure calls
        // this before any release or detection reads the failure.
        if !self.collection.observed_inputs.is_empty() {
            self.apply_observed_inputs();
        }
    }

    /// `compiler.md` §350 rule 5: a race observes `frame` from its call. An
    /// exception completion is marked now; a frame with no completion is
    /// marked when it stores a failure ([`Context::note_task_failure`]).
    pub(super) fn observe_aggregate_input(&mut self, frame: usize) {
        let Some(meta) = self.async_frames.get_mut(&frame) else {
            return;
        };
        match meta.completion.as_mut() {
            Some(crate::exception::Completion::Exception(payload)) => payload.observed = true,
            Some(crate::exception::Completion::Value(_)) => {}
            None if meta.observed_by_aggregate => {}
            None => {
                meta.observed_by_aggregate = true;
                let task_id = meta.task_id;
                if self.collection.observed_inputs.len() >= self.collection.observed_inputs_limit {
                    self.apply_observed_inputs();
                    self.collection.observed_inputs_limit = OBSERVED_INPUTS_MIN_LIMIT
                        .max(self.collection.observed_inputs.len().saturating_mul(2));
                }
                self.collection.observed_inputs.push((frame, task_id));
            }
        }
    }

    /// Marks the failure of each listed input that has one, and keeps the
    /// inputs that have no completion. The task id tells a reused address
    /// from the listed frame.
    fn apply_observed_inputs(&mut self) {
        let mut inputs = std::mem::take(&mut self.collection.observed_inputs);
        inputs.retain(
            |&(frame, task_id)| match self.async_frames.get_mut(&frame) {
                Some(meta) if meta.task_id == task_id => match meta.completion.as_mut() {
                    None => true,
                    Some(crate::exception::Completion::Exception(payload)) => {
                        payload.observed = true;
                        false
                    }
                    Some(crate::exception::Completion::Value(_)) => false,
                },
                _ => false,
            },
        );
        self.collection.observed_inputs = inputs;
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
        Self::serve_exit(
            self,
            |ctx| ctx,
            service,
            |ctx, kind| {
                ctx.run_own_collection(kind);
            },
        );
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
        Self::serve_exit(owner, context, service, |owner, kind| {
            Self::run_collection(owner, context, kind, &mut collect);
        });
    }

    /// The work of [`Context::finish_exit_service`]. `run` performs one
    /// collection of the class that it receives.
    fn serve_exit<T>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        service: ExitService,
        mut run: impl FnMut(&mut T, CollectionKind),
    ) {
        if !service.serve {
            return;
        }
        if service.detection {
            run(owner, CollectionKind::Detection);
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
            run(owner, CollectionKind::Automatic);
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
