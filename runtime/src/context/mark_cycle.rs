//! The mark of a collection in steps (`compiler.md` §356).
//!
//! A collection of the ship arena is a cycle: a start, mark steps, and an
//! end. Each of the three is a pause, and script code runs between them.
//! The start takes the roots into a work list. A step marks the blocks that
//! the words of the work list name, within a budget of words. The end is
//! one pause: it reads the roots again and marks with the budget. If the
//! work list is empty after the budget, the releases follow in the same
//! pause, and a sweep in steps starts (`compiler.md` §357). Else the pause
//! is a step, and the next pause tries the end again.

use super::collection::CollectionKind;
use super::*;

/// The smallest part of the headroom between two steps (`compiler.md`
/// §356 rule 6), so that a small limit does not make each allocation a
/// step.
const MIN_STEP_BYTES: usize = 4096;

/// The most steps of one cycle in mode 3 (`compiler.md` §356 rule 19).
/// The pause after them is the end, which empties the work list with no
/// budget.
pub(super) const STRESS_STEP_LIMIT: usize = 8;

/// The count of findings that the check of the mark keeps
/// (`compiler.md` §356 rule 20).
const CHECK_FINDINGS: usize = 8;

/// The scan of a large payload that a step did not complete
/// (`compiler.md` §356 rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DividedScan {
    payload: usize,
    class_id: u32,
    // The index of the next word to read, and the count of the words.
    next: usize,
    words: usize,
}

/// The state of a mark in steps in one Context.
#[derive(Debug)]
pub(super) struct MarkCycle {
    // The state word that an allocation writes in the header of a new
    // block: the marked word during a cycle (rule 4), else the live word.
    // compiler.md §357 rule 1: the live word of the exchange.
    pub(super) alloc_state: u64,
    // compiler.md §357 rule 1: the header state word of a live block and
    // the word that a mark writes. The end of a cycle and of a collection
    // of one pause exchanges them.
    pub(super) live_word: u64,
    pub(super) marked_word: u64,
    // compiler.md §357: the sweep in steps.
    pub(super) sweep: super::sweep_steps::SweepState,
    active: bool,
    work: Vec<usize>,
    divided: Option<DividedScan>,
    // The steps of the cycle in progress (rule 19).
    steps: usize,
    // True after a step that left the work list empty (rule 6a): the next
    // pause is the end, also if a store gave the work list a word since
    // that step. The end empties the list.
    end_next: bool,
    // True when the cycle is late (rule 7): the bytes requested since its
    // start exceed one limit. Each poll and each outermost exit of mode 1
    // then runs one step with the budget, until the end.
    late: bool,
    // The requested bytes at the start of the cycle (rule 7).
    start_requested: usize,
    // The references that the barrier received since the pending flag was
    // last set for them (rule 6b).
    given: usize,
    // The part of the headroom between two steps in mode 1 (rule 6).
    step_bytes: usize,
    // compiler.md §356 rule 20: when true, each end keeps the result of the
    // check in `checks`. Only tests set it.
    record_checks: bool,
    checks: Vec<MarkCheck>,
    // compiler.md §356 rule 20a: when true, each step and each end keeps
    // the result of the check of a pause in `pause_checks`. Only tests set
    // it.
    record_pause_checks: bool,
    pause_checks: Vec<MarkCheck>,
    // compiler.md §356 rule 5: true from the end of a mark that read the
    // roots and emptied the work list within the budget, until the
    // releases of the same pause start.
    end_marked: bool,
    // compiler.md §356 rule 20b: true for a Context that has a mark trace
    // target, which has no step form.
    traced: bool,
}

impl Default for MarkCycle {
    fn default() -> Self {
        Self {
            alloc_state: LIVE_STATE,
            live_word: LIVE_STATE,
            marked_word: MARK_STATE,
            sweep: super::sweep_steps::SweepState::default(),
            active: false,
            work: Vec::new(),
            divided: None,
            steps: 0,
            end_next: false,
            late: false,
            start_requested: 0,
            given: 0,
            step_bytes: MIN_STEP_BYTES,
            record_checks: false,
            checks: Vec::new(),
            record_pause_checks: false,
            pause_checks: Vec::new(),
            end_marked: false,
            traced: mark_trace_target().is_some(),
        }
    }
}

impl MarkCycle {
    fn has_work(&self) -> bool {
        !self.work.is_empty() || self.divided.is_some()
    }

    /// True if the next pause of the cycle is a step, not the end.
    fn step_is_next(&self) -> bool {
        self.has_work() && !self.end_next
    }

    /// `compiler.md` §356 rule 9: the release of a large payload during a
    /// mark stops its divided scan.
    pub(super) fn forget_divided(&mut self, payload: usize) {
        if self.divided.is_some_and(|scan| scan.payload == payload) {
            self.divided = None;
        }
    }
}

/// One reachable block with no mark (`compiler.md` §356 rule 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MarkFinding {
    /// The payload address of the block.
    pub(crate) block: usize,
    /// The class of the block.
    pub(crate) class_id: u32,
    /// The class of a block that holds it, or `None` for a root.
    pub(crate) holder_class: Option<u32>,
    /// The byte offset of the word in the payload of the holder.
    pub(crate) offset: usize,
}

/// The result of the check of the mark (`compiler.md` §356 rule 20) or of
/// the check of a pause (rule 20a).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MarkCheck {
    /// True for the check of rule 20a.
    pub(crate) pause: bool,
    /// Rule 20: the count of the reachable blocks with no mark. Rule 20a:
    /// the count of the words that break the invariant.
    pub(crate) count: u64,
    /// The first findings, at most eight. For rule 20a, `block` is the
    /// block with no mark that a word names.
    pub(crate) findings: Vec<MarkFinding>,
}

impl MarkCheck {
    /// The message of a failed check.
    pub(crate) fn message(&self) -> String {
        let mut message = if self.pause {
            format!(
                "compiler.md §356 rule 20a: {} words in blocks whose scan is complete name a live block with no mark and are not in the work list",
                self.count
            )
        } else {
            format!(
                "compiler.md §356 rule 20: {} reachable blocks have no mark at the end of a cycle",
                self.count
            )
        };
        for finding in &self.findings {
            let holder = finding
                .holder_class
                .map_or_else(|| "a root".to_owned(), |class| format!("class {class:#x}"));
            message.push_str(&format!(
                "; block {:#x} of class {:#x}, held by {holder} at offset {}",
                finding.block, finding.class_id, finding.offset
            ));
        }
        message
    }
}

impl Context {
    /// The byte offset of the one-byte flag "a mark is in progress" in the
    /// Context. Generated code loads the flag at this offset after a store
    /// of a reference (`compiler.md` §356 rule 12).
    #[must_use]
    pub const fn mark_in_progress_offset() -> usize {
        std::mem::offset_of!(Context, mark_in_progress)
    }

    /// True while a mark in steps is in progress (`compiler.md` §356 rule
    /// 12). It is never true for a Context with no step form (rule 21).
    #[must_use]
    pub fn mark_in_progress(&self) -> bool {
        self.mark_in_progress != 0
    }

    /// The budget of one mark step in words (`compiler.md` §356 rule 16).
    #[must_use]
    pub fn collection_step_budget(&self) -> usize {
        self.collection.settings.step_budget
    }

    /// Sets the budget of one mark step in words (`compiler.md` §356 rule
    /// 16). A budget of 0 makes each collection one pause. If a cycle is in
    /// progress, the new budget applies to its next step, and a budget of 0
    /// ends the cycle at the next poll. A Context with no step form keeps
    /// the budget, and the budget has no effect (rule 21). Returns false,
    /// with no change, while script depth is not zero.
    pub fn set_collection_step(&mut self, budget_words: usize) -> bool {
        if self.script_depth != 0 {
            return false;
        }
        let mut settings = self.collection.settings;
        settings.step_budget = budget_words;
        self.apply_collection_settings(settings);
        true
    }

    /// A host-timed pause of a cycle (`compiler.md` §356 rule 18). If no
    /// cycle is in progress, it starts one. If the work list is not empty,
    /// it runs one step of `budget_words`. If the work list is empty, or
    /// the last step left it empty, it runs the end, whose mark has the
    /// budget (rule 5): if the mark does not complete within the budget,
    /// the call is a step and returns 1. After the end, each call runs one
    /// sweep step of at most 5 slots for each word of `budget_words`
    /// (`compiler.md` §357 rules 5 and 12). Returns 1 if a cycle or a sweep
    /// is in progress after the call, 0 if the call completed the sweep,
    /// and -1 with no work for a script depth that is not zero, a budget of
    /// 0, or a mode other than 0 and 1.
    ///
    /// A Context with no step form runs one whole collection and returns 0
    /// (rules 20b and 21).
    pub fn collect_step(&mut self, budget_words: usize) -> i32 {
        if self.script_depth != 0
            || budget_words == 0
            || !matches!(
                self.collection.settings.mode,
                CollectionMode::Off | CollectionMode::Automatic
            )
        {
            return -1;
        }
        if !self.has_step_form() {
            self.run_whole_collection(CollectionKind::Automatic);
            return 0;
        }
        i32::from(self.run_cycle_pause(budget_words))
    }

    /// The barrier of `compiler.md` §356 rule 10: after a store of the word
    /// `value` into a block, the word goes to the work list if a mark is in
    /// progress.
    #[inline]
    pub fn mark_store(&mut self, value: usize) {
        if self.mark_in_progress != 0 && value != 0 {
            self.give_word(value);
        }
    }

    /// The header of the block whose payload is at `payload`, if it is a
    /// block of this Context.
    fn block_header(&self, payload: usize) -> Option<*mut u8> {
        self.arena_lookup_block(payload)
            .map(|(block, _)| block)
            .or_else(|| self.large.get(&payload).map(|allocation| allocation.base))
    }

    /// `compiler.md` §356 rule 10: the barrier gives `word` to the work
    /// list only if it names a live block with no mark that is not in the
    /// list. That block gets the state "in the work list", so the list is
    /// not larger than the count of the blocks. Rule 6b: each reference
    /// that the barrier receives counts, also one that it does not give.
    /// A reference that is not given can be in the list already, so the
    /// count keeps the steps going while the program stores, and the list
    /// drains.
    #[cold]
    fn give_word(&mut self, word: usize) {
        self.count_given_reference();
        let Some(block) = self.block_header(word) else {
            return;
        };
        // SAFETY: an exact allocation lookup proves that the header is
        // readable and writable.
        unsafe {
            if block.cast::<u64>().read() != self.mark_cycle.live_word {
                // Dead, free-listed, marked, or in the list.
                return;
            }
            block.cast::<u64>().write(GREY_STATE);
        }
        self.mark_cycle.work.push(word);
        self.collection.stats.carry_words(1);
    }

    /// `compiler.md` §356 rule 6b: the mark work follows the stores. After
    /// each budget of references that the barrier received, the next poll
    /// or outermost exit of mode 1 runs one pause.
    fn count_given_reference(&mut self) {
        self.mark_cycle.given += 1;
        let budget = self.collection.settings.step_budget;
        if budget != 0 && self.mark_cycle.given >= budget {
            self.mark_cycle.given = 0;
            if self.collection.settings.mode == CollectionMode::Automatic {
                self.collection_pending = 1;
            }
        }
    }

    /// The barrier of `compiler.md` §356 rule 14 for a whole block: if a
    /// mark is in progress, the block at `payload` goes to the work list
    /// again, so that a step reads its payload again. A block that is in
    /// the work list already stays there once (rule 10). This is the form
    /// for a block that the mark gave the mark and never read: a block of
    /// rule 4 that the runtime or generated code filled.
    ///
    /// Rule 14a: the block of the divided scan keeps its position, and the
    /// words of the part that the scan read go to the work list.
    pub fn mark_block_again(&mut self, payload: usize) {
        if self.mark_in_progress == 0 || payload == 0 {
            return;
        }
        if self.is_divided(payload) {
            self.mark_moved_words(payload, 0, usize::MAX, None);
            return;
        }
        self.count_given_reference();
        let Some(block) = self.block_header(payload) else {
            return;
        };
        // SAFETY: an exact allocation lookup proves that the header is
        // readable and writable.
        unsafe {
            let state = block.cast::<u64>().read();
            if state != self.mark_cycle.live_word && state != self.mark_cycle.marked_word {
                // In the list already, or not live.
                return;
            }
            block.cast::<u64>().write(GREY_STATE);
        }
        self.mark_cycle.work.push(payload);
        self.collection.stats.carry_words(1);
    }

    /// True if `payload` is the block of the divided scan (`compiler.md`
    /// §356 rule 3).
    fn is_divided(&self, payload: usize) -> bool {
        self.mark_cycle
            .divided
            .is_some_and(|scan| scan.payload == payload)
    }

    /// `compiler.md` §356 rule 14a: after the runtime moved words inside
    /// the block at `payload`, into the bytes `from..to` of its payload. If
    /// each word of the range came from `distance` bytes higher, `distance`
    /// is `Some`; if a word can come from any position, it is `None`. A
    /// move adds no reference to the block. So a block whose scan is
    /// complete, and a block whose scan did not start, need no work. For
    /// the block of the divided scan, the position stays, and each word of
    /// the range in the part that the scan read, which can hold a word from
    /// the part that it did not read, goes to the work list. The cost has
    /// the order of the move.
    pub(crate) fn mark_moved_words(
        &mut self,
        payload: usize,
        from: usize,
        to: usize,
        distance: Option<usize>,
    ) {
        if self.mark_in_progress == 0 {
            return;
        }
        let Some(scan) = self
            .mark_cycle
            .divided
            .filter(|scan| scan.payload == payload)
        else {
            return;
        };
        let read = scan.next * 8;
        // A word at `at` came from `at + distance`, so it came from the part
        // that the scan did not read if `at + distance >= read`.
        let low = distance.map_or(from, |distance| from.max(read.saturating_sub(distance)));
        let high = to.min(read);
        if low >= high {
            return;
        }
        let low = low & !7;
        self.give_written_words((payload + low) as *const u8, high - low);
    }

    /// `compiler.md` §356 rule 14: after the runtime wrote `bytes` bytes at
    /// `at` inside a block, each word of the range goes to the work list if
    /// a mark is in progress. With no mark the cost is one test of the flag.
    #[inline]
    pub(crate) fn mark_written_words(&mut self, at: *const u8, bytes: usize) {
        if self.mark_in_progress != 0 {
            self.give_written_words(at, bytes);
        }
    }

    /// `compiler.md` §356 rule 14: after the runtime filled the block at
    /// `payload`, which the mark gave the mark and never read (rule 4), the
    /// block goes to the work list again if a mark is in progress.
    #[inline]
    pub(crate) fn mark_written_block(&mut self, payload: *const u8) {
        if self.mark_in_progress != 0 {
            self.mark_block_again(payload as usize);
        }
    }

    /// `compiler.md` §356 rule 14 for the element storage of an array: after
    /// the runtime wrote the elements `from..to` of the array at `handle`,
    /// whose storage was at `data_before` before the write. If the storage
    /// moved (a growth), the new storage goes to the work list again. Else
    /// the words of the written elements go to the work list. An array whose
    /// elements hold no handle needs no work (§344 rule 1).
    ///
    /// # Safety
    ///
    /// `handle` is a live array payload of this Context.
    #[inline]
    pub(crate) unsafe fn mark_written_elements(
        &mut self,
        handle: *const u8,
        data_before: *const u8,
        from: usize,
        to: usize,
    ) {
        if self.mark_in_progress != 0 {
            // SAFETY: forwarded caller contract.
            unsafe { self.give_written_elements(handle, data_before, from, to) };
        }
    }

    /// `compiler.md` §356 rule 14a: after the runtime moved elements inside
    /// the storage of the array at `handle` (`sort`, `reverse`, `splice`,
    /// `copyWithin`, `shift`), into the elements `from..to`. If each element
    /// of the range came from `distance` elements higher, `distance` is
    /// `Some`; else it is `None`. See [`Context::mark_moved_words`].
    ///
    /// # Safety
    ///
    /// `handle` is a live array payload of this Context.
    #[inline]
    pub(crate) unsafe fn mark_moved_elements(
        &mut self,
        handle: *const u8,
        from: usize,
        to: usize,
        distance: Option<usize>,
    ) {
        if self.mark_in_progress != 0 {
            // SAFETY: forwarded caller contract.
            unsafe { self.give_moved_elements(handle, from, to, distance) };
        }
    }

    /// `compiler.md` §356 rule 14: after the runtime wrote references into
    /// the storage of the array at `handle`, which this call allocated, the
    /// storage goes to the work list. It has the mark (rule 4), and no step
    /// read it.
    ///
    /// # Safety
    ///
    /// `handle` is a live array payload of this Context.
    #[inline]
    pub(crate) unsafe fn mark_new_elements(&mut self, handle: *const u8) {
        if self.mark_in_progress != 0 {
            // SAFETY: forwarded caller contract. A null storage address
            // differs from each live storage, so the storage goes again.
            unsafe { self.give_written_elements(handle, std::ptr::null(), 0, 0) };
        }
    }

    /// `compiler.md` §356 rule 14: after the runtime set the completion of
    /// the task `frame`, its words go to the work list if a mark is in
    /// progress. The frame can have its mark already, and the end reads the
    /// completion of a completed task only through its frame (§348 rule 2).
    pub(crate) fn mark_written_completion(&mut self, frame: usize) {
        if self.mark_in_progress == 0 {
            return;
        }
        let mut words = Vec::new();
        self.push_frame_completion(frame, &mut words);
        for word in words {
            if word != 0 {
                self.give_word(word);
            }
        }
    }

    #[cold]
    fn give_written_words(&mut self, at: *const u8, bytes: usize) {
        if at.is_null() || bytes == 0 {
            return;
        }
        // A reference is one aligned word of a payload, so the words that
        // lie whole inside the range are the words that can hold one.
        let start = (at as usize).saturating_add(7) & !7;
        let end = (at as usize).saturating_add(bytes) & !7;
        let mut address = start;
        while address < end {
            // SAFETY: the caller wrote the range, so each whole aligned word
            // inside it is readable.
            let word = unsafe { (address as *const usize).read() };
            if word != 0 {
                self.give_word(word);
            }
            address += 8;
        }
    }

    #[cold]
    unsafe fn give_moved_elements(
        &mut self,
        handle: *const u8,
        from: usize,
        to: usize,
        distance: Option<usize>,
    ) {
        if handle.is_null() {
            return;
        }
        // SAFETY: caller contract.
        if unsafe { self.array_elements(handle) } == ArrayElements::NoHandle {
            return;
        }
        // SAFETY: caller contract.
        let header = unsafe { &*handle.cast::<ArrayHeader>() };
        if header.data.is_null() {
            return;
        }
        let elem = header.elem_size as usize;
        self.mark_moved_words(
            header.data as usize,
            from.saturating_mul(elem),
            to.saturating_mul(elem),
            distance.map(|distance| distance.saturating_mul(elem)),
        );
    }

    #[cold]
    unsafe fn give_written_elements(
        &mut self,
        handle: *const u8,
        data_before: *const u8,
        from: usize,
        to: usize,
    ) {
        if handle.is_null() {
            return;
        }
        // SAFETY: caller contract.
        if unsafe { self.array_elements(handle) } == ArrayElements::NoHandle {
            return;
        }
        // SAFETY: caller contract.
        let header = unsafe { &*handle.cast::<ArrayHeader>() };
        if header.data.is_null() {
            return;
        }
        if header.data.cast_const() != data_before {
            self.mark_block_again(header.data as usize);
            return;
        }
        let elem = header.elem_size as usize;
        let to = to.min(header.len as usize);
        if from >= to {
            return;
        }
        // SAFETY: the elements `from..to` are inside the storage.
        let at = unsafe { header.data.add(from * elem) };
        self.give_written_words(at, (to - from) * elem);
    }

    /// True while a cycle is in progress (`compiler.md` §356 rule 1).
    pub(super) fn mark_cycle_active(&self) -> bool {
        self.mark_cycle.active
    }

    /// True when a collection of this Context can run in steps: the ship
    /// arena with no mark trace target (`compiler.md` §356 rules 20b and
    /// 21).
    fn has_step_form(&self) -> bool {
        self.uses_ship_arena() && !self.mark_cycle.traced
    }

    /// True when a poll of this Context runs the pauses of a cycle: the
    /// step form with a budget above 0 (`compiler.md` §356 rules 16, 20b,
    /// and 21).
    pub(super) fn has_poll_step_form(&self) -> bool {
        self.has_step_form() && self.collection.settings.step_budget > 0
    }

    /// The requested bytes above which an allocation sets the pending flag
    /// when no cycle is in progress (`compiler.md` §356 rule 7). In mode 1
    /// of the step form, a cycle starts at 7/8 of the limit, so that its
    /// mark can end near the limit. Else it is the limit of §346.
    pub(super) fn cycle_start_threshold(&self) -> usize {
        let limit = self.collection.limit;
        if self.has_poll_step_form() && self.collection.settings.mode == CollectionMode::Automatic {
            limit - limit / 8
        } else {
            limit
        }
    }

    /// Runs one pause of a cycle: the start, a step of `budget` words, or
    /// the end. Returns true if a cycle or a sweep is in progress after the
    /// pause.
    ///
    /// `compiler.md` §357 rules 4, 9, and 12: while a sweep is in progress,
    /// the pause is a sweep step, and no cycle starts. The end of a cycle
    /// starts a sweep in steps.
    pub(super) fn run_cycle_pause(&mut self, budget: usize) -> bool {
        if self.sweep_in_progress() {
            return self.sweep_pause(budget);
        }
        if !self.mark_cycle.active {
            self.start_cycle(budget);
            return true;
        }
        // compiler.md §356 rule 19: after the limit of steps in mode 3, the
        // next pause is the end, which empties the work list with no budget.
        // The limit is not the limit of rule 7, so `forced_ends` does not
        // count it.
        let stress_limit = self.collection.settings.mode == CollectionMode::StepStress
            && self.mark_cycle.steps >= STRESS_STEP_LIMIT;
        if stress_limit {
            self.check_at_pause();
            self.end_cycle_with_sweep_in_steps(budget);
            return self.sweep_in_progress();
        }
        if self.mark_cycle.step_is_next() {
            self.note_late_cycle();
            self.mark_step(budget);
            self.mark_cycle.end_next = !self.mark_cycle.has_work();
            self.check_at_pause();
            self.place_next_pause();
            return true;
        }
        self.check_at_pause();
        // compiler.md §356 rule 7: a cycle can become late during the tries
        // of the end also.
        self.note_late_cycle();
        // compiler.md §356 rule 5: the end reads the roots and marks with
        // the budget. If the work list is not empty after the budget, the
        // pause is a step, and the next pause tries the end again.
        let started = self.collection.stats.start();
        let words = self.end_mark(budget) as u64;
        if self.mark_cycle.has_work() {
            self.mark_cycle.steps += 1;
            self.collection.stats.finish_pause(started, words, true);
            self.check_at_pause();
            self.place_next_pause();
            return true;
        }
        // The releases, the large blocks, and the exchange follow in the
        // same pause, which keeps the time and the words of the mark. The
        // small blocks follow in sweep steps (compiler.md §357 rule 4).
        self.mark_cycle.end_marked = true;
        self.collection.stats.continue_pause(started, words);
        self.end_cycle_with_sweep_in_steps(budget);
        self.mark_cycle.end_marked = false;
        self.sweep_in_progress()
    }

    /// The mark of an end (`compiler.md` §356 rule 5): the roots go to the
    /// work list as the barrier gives a word (rule 10), and a mark of
    /// `budget` words follows. Returns the words that the pause read: one
    /// for each root word, and the words of the mark.
    fn end_mark(&mut self, budget: usize) -> usize {
        let tasks = self.completed_tasks();
        let mut roots = Vec::new();
        self.push_roots(&mut roots, &mut None, &tasks);
        for &word in &roots {
            let Some(block) = self.block_header(word) else {
                continue;
            };
            // SAFETY: an exact allocation lookup proves that the header is
            // readable and writable.
            unsafe {
                if block.cast::<u64>().read() != self.mark_cycle.live_word {
                    // Dead, free-listed, marked, or in the list.
                    continue;
                }
                block.cast::<u64>().write(GREY_STATE);
            }
            self.mark_cycle.work.push(word);
        }
        roots.len() + self.mark_with_budget(budget)
    }

    /// `compiler.md` §356 rule 5: true once, after an end whose mark
    /// emptied the work list within the budget. The releases of that end
    /// then do not read the roots again.
    pub(super) fn take_end_marked(&mut self) -> bool {
        std::mem::take(&mut self.mark_cycle.end_marked)
    }

    /// `compiler.md` §356 rule 5: after the mark of an end, the flag of rule
    /// 12 clears before the releases, so that no barrier gives a word to a
    /// work list that the end discards.
    pub(super) fn close_cycle_mark(&mut self) {
        if self.mark_cycle.active {
            self.mark_in_progress = 0;
        }
    }

    /// The start of a cycle (`compiler.md` §356 rule 1): the roots go to
    /// the work list, and the flag of rule 12 is set.
    fn start_cycle(&mut self, budget: usize) {
        let started = self.collection.stats.start();
        let tasks = self.completed_tasks();
        let mut work = std::mem::take(&mut self.mark_cycle.work);
        work.clear();
        self.push_roots(&mut work, &mut None, &tasks);
        let words = work.len() as u64;
        self.mark_cycle.work = work;
        self.mark_cycle.divided = None;
        self.mark_cycle.steps = 0;
        self.mark_cycle.end_next = false;
        self.mark_cycle.late = false;
        self.mark_cycle.given = 0;
        self.mark_cycle.active = true;
        self.mark_cycle.alloc_state = self.mark_cycle.marked_word;
        self.mark_in_progress = 1;
        self.mark_cycle.start_requested = self.collection.requested;
        // compiler.md §356 rule 6: the steps share the headroom that is left
        // before the limit. The live words estimate the work of the mark.
        let left = self
            .collection
            .limit
            .saturating_sub(self.collection.requested);
        let steps = (self.live_bytes_counter / 8).div_ceil(budget.max(1)).max(1);
        self.mark_cycle.step_bytes = (left / steps).max(MIN_STEP_BYTES);
        self.collection.stats.finish_pause(started, words, false);
        self.place_next_pause();
    }

    /// `compiler.md` §356 rule 7: past one limit of requested bytes since the
    /// start, the cycle is late, and the record counts it one time. A step
    /// of a late cycle keeps the budget.
    fn note_late_cycle(&mut self) {
        if self.mark_cycle.late {
            return;
        }
        let since_start = self
            .collection
            .requested
            .saturating_sub(self.mark_cycle.start_requested);
        if since_start > self.collection.limit {
            self.mark_cycle.late = true;
            self.collection.stats.count_forced_end();
        }
    }

    /// `compiler.md` §356 rule 6: in mode 1 the allocation sets the pending
    /// flag after each part of the headroom. If the next pause is the end,
    /// the next poll runs it. In mode 2 and mode 3 the flag stays set; in
    /// mode 0 the host runs each pause.
    fn place_next_pause(&mut self) {
        match self.collection.settings.mode {
            CollectionMode::Automatic => {
                // compiler.md §356 rule 7: a late cycle runs one step at
                // each poll and each outermost exit.
                if self.mark_cycle.step_is_next() && !self.mark_cycle.late {
                    self.collection.pending_at = self
                        .collection
                        .requested
                        .saturating_add(self.mark_cycle.step_bytes);
                    self.collection_pending = 0;
                } else {
                    self.collection_pending = 1;
                }
            }
            CollectionMode::Off => self.collection_pending = 0,
            CollectionMode::EachPoll | CollectionMode::StepStress => self.collection_pending = 1,
        }
    }

    /// One mark step (`compiler.md` §356 rules 2 and 3).
    fn mark_step(&mut self, budget: usize) {
        self.mark_cycle.steps += 1;
        let started = self.collection.stats.start();
        let read = self.mark_with_budget(budget);
        self.collection
            .stats
            .finish_pause(started, read as u64, true);
    }

    /// The mark of one pause (`compiler.md` §356 rules 2 and 3). It stops
    /// when the words that entered the work list reach `budget`, or when it
    /// tested `budget` words of the list. It reads at most the budget plus
    /// the words of one small block: a large payload is read in parts.
    /// Returns the words that it read.
    fn mark_with_budget(&mut self, budget: usize) -> usize {
        let mut work = std::mem::take(&mut self.mark_cycle.work);
        let mut read = 0usize;
        let mut tested = 0usize;
        while read < budget && tested < budget {
            if let Some(mut scan) = self.mark_cycle.divided {
                let count = (scan.words - scan.next).min(budget - read);
                // SAFETY: the large record of the payload exists, because
                // its release ends the scan (rule 9); `next + count` is not
                // above the words of the payload.
                let part = unsafe { (scan.payload as *const u8).add(scan.next * 8) };
                read += self.scan_payload(part, count * 8, scan.class_id, &mut work, &mut None);
                scan.next += count;
                self.mark_cycle.divided = (scan.next < scan.words).then_some(scan);
                continue;
            }
            let Some(address) = work.pop() else {
                break;
            };
            tested += 1;
            read += self.mark_one(address, &mut work);
        }
        self.mark_cycle.work = work;
        read
    }

    /// Marks the block that `address` names, if it is a live block with no
    /// mark. A small payload is read whole; a large payload starts a divided
    /// scan. Returns the words that went to the work list.
    fn mark_one(&mut self, address: usize, work: &mut Vec<usize>) -> usize {
        let (block, payload_size, large) =
            if let Some((block, class)) = self.arena_lookup_block(address) {
                (block, (SMALLEST_BLOCK << class) - HEADER_SIZE, false)
            } else if let Some(allocation) = self.large.get(&address) {
                (allocation.base, allocation.payload_size, true)
            } else {
                return 0;
            };
        // SAFETY: `block` heads an owned allocation with a complete header
        // and at least `payload_size` payload bytes.
        let class_id = unsafe {
            // compiler.md §356 rule 10: a block in the work list has no
            // mark yet.
            let state = block.cast::<u64>().read();
            if state != self.mark_cycle.live_word && state != GREY_STATE {
                // Dead, free-listed, or marked.
                return 0;
            }
            block.cast::<u64>().write(self.mark_cycle.marked_word);
            header_class_id(block)
        };
        let mut pushed = 0;
        if class_id == CLASS_GENERATOR {
            pushed += self.push_frame_completion(address, work);
        }
        if large {
            if !class_holds_no_handle(class_id) && payload_size >= 8 {
                self.mark_cycle.divided = Some(DividedScan {
                    payload: address,
                    class_id,
                    next: 0,
                    words: payload_size / 8,
                });
            }
            return pushed;
        }
        pushed
            + self.scan_payload(
                block.wrapping_add(HEADER_SIZE),
                payload_size,
                class_id,
                work,
                &mut None,
            )
    }

    /// `compiler.md` §348 rule 2 in a step: the mark reached the frame of a
    /// task, and its completion goes to the work list. A frame that is not
    /// a completed task gives its completion as a root at the end, so a
    /// step gives the completion of each reached frame.
    pub(super) fn push_frame_completion(&self, frame: usize, work: &mut Vec<usize>) -> usize {
        let Some(completion) = self
            .async_frames
            .get(&frame)
            .and_then(|meta| meta.completion.as_ref())
        else {
            return 0;
        };
        let before = work.len();
        if let Some(bytes) = completion.value() {
            for chunk in bytes.chunks_exact(core::mem::size_of::<usize>()) {
                // SAFETY: `chunk` is one native word of Context-owned
                // completion storage; an unaligned read keeps aggregate
                // layouts.
                work.push(unsafe { chunk.as_ptr().cast::<usize>().read_unaligned() });
            }
        }
        if let Some(object) = completion.exception_object() {
            work.push(object);
        }
        work.len() - before
    }

    /// `compiler.md` §356 rules 1 and 8: an end with no budget takes the
    /// work list of the steps into the mark of one pause. The divided scan
    /// gives the words of the part that it did not read, so the end does not
    /// read the part that the steps read again. Returns the count of the
    /// words of the steps.
    pub(super) fn take_cycle_work(&mut self, work: &mut Vec<usize>) -> usize {
        if !self.mark_cycle.active {
            return 0;
        }
        let carried = self.mark_cycle.work.len();
        work.append(&mut self.mark_cycle.work);
        if let Some(scan) = self.mark_cycle.divided.take() {
            if self.large.contains_key(&scan.payload) {
                // SAFETY: the large record owns the payload, and `next` is
                // not above the words of the payload.
                let part = unsafe { (scan.payload as *const u8).add(scan.next * 8) };
                self.scan_payload(
                    part,
                    (scan.words - scan.next) * 8,
                    scan.class_id,
                    work,
                    &mut None,
                );
            }
        }
        carried
    }

    /// `compiler.md` §356 rule 5 and §357 rule 1: after the exchange of the
    /// two words a block that the mark reached holds the live word, and the
    /// cycle ends. A new block gets the live word.
    pub(super) fn clear_mark_cycle(&mut self) {
        if !self.mark_cycle.active {
            return;
        }
        self.mark_cycle.active = false;
        self.mark_cycle.alloc_state = self.mark_cycle.live_word;
        self.mark_in_progress = 0;
        self.mark_cycle.work = Vec::new();
        self.mark_cycle.divided = None;
        self.mark_cycle.end_marked = false;
    }

    /// `compiler.md` §356 rules 19 and 20: at the end of a cycle, after the
    /// mark and before the sweep, mode 3 runs the check and stops the
    /// process with a message if it fails.
    pub(super) fn check_at_cycle_end(&mut self) {
        if !self.mark_cycle.active {
            return;
        }
        let stress = self.collection.settings.mode == CollectionMode::StepStress;
        if !stress && !self.mark_cycle.record_checks {
            return;
        }
        let check = self.mark_check();
        if stress && check.count != 0 {
            eprintln!("{}", check.message());
            std::process::abort();
        }
        if self.mark_cycle.record_checks {
            self.mark_cycle.checks.push(check);
        }
    }

    /// The check of the mark (`compiler.md` §356 rule 20). A separate walk
    /// from the roots finds each reachable block with a second record, not
    /// with the marks. It reports each block of that record with no mark.
    /// The walk reads a payload as the mark does, so it is conservative.
    pub(crate) fn mark_check(&self) -> MarkCheck {
        let tasks = self.completed_tasks();
        let mut roots = Vec::new();
        self.push_roots(&mut roots, &mut None, &tasks);
        let mut work: Vec<(usize, Option<u32>, usize)> = roots
            .into_iter()
            .map(|address| (address, None, 0))
            .collect();
        let mut seen: HashSet<usize, BuildHasherDefault<AddressHasher>> = HashSet::default();
        let mut check = MarkCheck::default();
        while let Some((address, holder_class, offset)) = work.pop() {
            let found = if let Some((block, class)) = self.arena_lookup_block(address) {
                Some((block, (SMALLEST_BLOCK << class) - HEADER_SIZE))
            } else {
                self.large
                    .get(&address)
                    .map(|allocation| (allocation.base, allocation.payload_size))
            };
            let Some((block, payload_size)) = found else {
                continue;
            };
            // SAFETY: an exact allocation lookup proves that the header is
            // readable.
            let (state, class_id) = unsafe { (block.cast::<u64>().read(), header_class_id(block)) };
            if !self.header_is_live(state) || !seen.insert(address) {
                continue;
            }
            // A block in the work list at the end has no mark.
            if state != self.mark_cycle.marked_word {
                check.count += 1;
                if check.findings.len() < CHECK_FINDINGS {
                    check.findings.push(MarkFinding {
                        block: address,
                        class_id,
                        holder_class,
                        offset,
                    });
                }
            }
            if class_id == CLASS_GENERATOR {
                let mut words = Vec::new();
                if tasks.contains(address) {
                    self.push_frame_completion(address, &mut words);
                }
                work.extend(words.into_iter().map(|word| (word, Some(class_id), 0)));
            }
            if class_holds_no_handle(class_id) {
                continue;
            }
            for index in 0..payload_size / 8 {
                // SAFETY: the word is inside the owned payload.
                let word = unsafe {
                    (address as *const u8)
                        .add(index * 8)
                        .cast::<usize>()
                        .read_unaligned()
                };
                work.push((word, Some(class_id), index * 8));
            }
        }
        check
    }

    /// `compiler.md` §356 rule 20a: after each step and before each end,
    /// mode 3 runs the check of a pause and stops the process with a
    /// message if it fails.
    fn check_at_pause(&mut self) {
        if !self.mark_cycle.active {
            return;
        }
        let stress = self.collection.settings.mode == CollectionMode::StepStress;
        if !stress && !self.mark_cycle.record_pause_checks {
            return;
        }
        let check = self.mark_pause_check();
        if stress && check.count != 0 {
            eprintln!("{}", check.message());
            std::process::abort();
        }
        if self.mark_cycle.record_pause_checks {
            self.mark_cycle.pause_checks.push(check);
        }
    }

    /// The check of a pause (`compiler.md` §356 rule 20a). Each live block
    /// that has its mark and whose scan is complete is read as the mark
    /// reads it. Each word that names a live block with no mark must be in
    /// the work list: the word itself, or the block has the state "in the
    /// work list" (rule 10). The block of the divided scan is not complete,
    /// and a block that is in the work list again has no mark.
    pub(crate) fn mark_pause_check(&self) -> MarkCheck {
        let mut check = MarkCheck {
            pause: true,
            ..MarkCheck::default()
        };
        if !self.mark_cycle.active {
            return check;
        }
        let listed: HashSet<usize, BuildHasherDefault<AddressHasher>> =
            self.mark_cycle.work.iter().copied().collect();
        let divided = self.mark_cycle.divided.map(|scan| scan.payload);
        let unmarked_class = |word: usize| -> Option<u32> {
            let block = if let Some((block, _)) = self.arena_lookup_block(word) {
                block
            } else {
                self.large.get(&word)?.base
            };
            // SAFETY: an exact allocation lookup proves that the header is
            // readable.
            unsafe {
                (block.cast::<u64>().read() == self.mark_cycle.live_word)
                    .then(|| header_class_id(block))
            }
        };
        let test = |word: usize, holder: u32, offset: usize, check: &mut MarkCheck| {
            if word == 0 || listed.contains(&word) {
                return;
            }
            if let Some(class_id) = unmarked_class(word) {
                check.count += 1;
                if check.findings.len() < CHECK_FINDINGS {
                    check.findings.push(MarkFinding {
                        block: word,
                        class_id,
                        holder_class: Some(holder),
                        offset,
                    });
                }
            }
        };
        let arena = self.live_blocks().map(|(block, size)| {
            (
                block,
                block.wrapping_add(HEADER_SIZE) as usize,
                size - HEADER_SIZE,
            )
        });
        let large = self
            .large
            .iter()
            .map(|(&payload, allocation)| (allocation.base, payload, allocation.payload_size));
        for (block, payload, payload_size) in arena.chain(large) {
            // SAFETY: each block of the walk has a complete header.
            let (state, class_id) = unsafe { (block.cast::<u64>().read(), header_class_id(block)) };
            if state != self.mark_cycle.marked_word || divided == Some(payload) {
                continue;
            }
            if class_id == CLASS_GENERATOR {
                let mut words = Vec::new();
                self.push_frame_completion(payload, &mut words);
                for word in words {
                    test(word, class_id, 0, &mut check);
                }
            }
            if class_holds_no_handle(class_id) {
                continue;
            }
            for index in 0..payload_size / 8 {
                // SAFETY: the word is inside the owned payload.
                let word = unsafe {
                    (payload as *const u8)
                        .add(index * 8)
                        .cast::<usize>()
                        .read_unaligned()
                };
                test(word, class_id, index * 8, &mut check);
            }
        }
        check
    }

    /// Makes each step and each end keep the result of the check of a pause
    /// (`compiler.md` §356 rule 20a).
    #[cfg(test)]
    pub(crate) fn record_mark_pause_checks(&mut self) {
        self.mark_cycle.record_pause_checks = true;
    }

    /// The results of the checks of a pause that the steps and ends kept.
    #[cfg(test)]
    pub(crate) fn mark_pause_checks(&self) -> &[MarkCheck] {
        &self.mark_cycle.pause_checks
    }

    /// Removes each copy of `word` from the work list, and keeps the state
    /// of its block. A test builds a block that is in the list by its state
    /// only (`compiler.md` §356 rule 20a).
    #[cfg(test)]
    pub(crate) fn remove_mark_work_word(&mut self, word: usize) {
        self.mark_cycle.work.retain(|listed| *listed != word);
    }

    /// Makes each end of a cycle keep the result of the check of the mark
    /// (`compiler.md` §356 rule 20).
    #[cfg(test)]
    pub(crate) fn record_mark_checks(&mut self) {
        self.mark_cycle.record_checks = true;
    }

    /// The results of the checks that the ends of the cycles kept.
    #[cfg(test)]
    pub(crate) fn mark_checks(&self) -> &[MarkCheck] {
        &self.mark_cycle.checks
    }

    /// The divided scan in progress: its payload, the index of the next
    /// word, and the count of the words of the payload.
    #[cfg(test)]
    pub(crate) fn mark_divided_scan(&self) -> Option<(usize, usize, usize)> {
        self.mark_cycle
            .divided
            .map(|scan| (scan.payload, scan.next, scan.words))
    }

    /// The count of the words in the work list and in a divided scan.
    #[cfg(test)]
    pub(crate) fn mark_work_words(&self) -> usize {
        self.mark_cycle.work.len()
            + self
                .mark_cycle
                .divided
                .map_or(0, |scan| scan.words - scan.next)
    }
}
