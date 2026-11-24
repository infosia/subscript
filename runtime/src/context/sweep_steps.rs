//! The sweep of a collection in steps (`compiler.md` §357).
//!
//! A Context of the ship arena holds two header state words: the live word
//! and the marked word. A mark writes the marked word. At the end of a cycle,
//! and of a collection of one pause, the Context exchanges the two words, so
//! a block that the mark reached is live with no write, and a block with the
//! former live word is a block to release (rule 1). The end of a cycle in
//! steps does not sweep the small blocks (rule 4). Sweep steps release the
//! blocks to release, each in a part of the arena that starts at a position
//! that the Context keeps (rule 5). A collection that must be exact, and a
//! collection of one pause, sweep the whole arena in their own pause.

use super::collection::CollectionKind;
use super::mark_cycle::MarkFinding;
use super::*;

/// The most slots that a sweep step reads for each word of the budget
/// (`compiler.md` §357 rule 5).
const SLOTS_PER_WORD: usize = 5;

/// The smallest part of the requested bytes between two sweep steps
/// (`compiler.md` §357 rule 6).
const MIN_PART_BYTES: usize = 4096;

/// The sweep ends at this fraction, one over the divisor, of the requested
/// bytes that are left before the start of the next cycle (`compiler.md`
/// §357 rule 6).
const SWEEP_END_DIVISOR: usize = 8;

/// The most sweep steps of one sweep in mode 3 (`compiler.md` §357 rule
/// 15). A step of mode 3 reads 5 slots for each word of the budget, or the
/// slots of the sweep over this count if that is more, so that a new cycle
/// starts after a bounded count of polls.
const STRESS_SWEEP_STEPS: usize = 8;

/// The count of findings that the check of a sweep keeps (`compiler.md`
/// §357 rule 15).
const CHECK_FINDINGS: usize = 8;

/// The state of the sweep of one Context (`compiler.md` §357).
#[derive(Debug, Default)]
pub(super) struct SweepState {
    // True from the exchange of rule 1 until the sweep reached each slot.
    active: bool,
    // compiler.md §357 rule 4: set by the end of a cycle in steps, so that
    // the end does not sweep the small blocks.
    in_steps: bool,
    // The bump of each chunk at the exchange. A slot at or above it holds
    // a block that was allocated after the exchange, which is live.
    limits: Vec<usize>,
    // The position of the next sweep step: a chunk and a slot in it.
    chunk: usize,
    slot: usize,
    // The slots below the limits that the sweep did not read.
    left: usize,
    // The slots below the limits at the exchange.
    total: usize,
    // The live bytes at the exchange, and the payload bytes of the blocks
    // to release that the sweep or the program released since then. Their
    // difference is the live bytes of a sweep of one pause (rule 11).
    base_at_end: usize,
    released_bytes: usize,
    // compiler.md §357 rule 6: the part of the requested bytes between two
    // sweep steps in mode 1.
    part_bytes: usize,
    // compiler.md §357 rule 9: true when the requested bytes reached the
    // start of the next cycle before the sweep ended.
    late: bool,
    // When true, the completion of each sweep keeps the result of the check
    // of rule 15 in `checks`. Only tests set it.
    record_checks: bool,
    checks: Vec<SweepCheck>,
    // The payloads that this sweep released, for the check of rule 15.
    // Present only while the check runs at the completion.
    released: Option<AddressSet>,
}

/// The result of the check of a sweep (`compiler.md` §357 rule 15).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SweepCheck {
    /// The blocks that the sweep released and that a walk from the roots
    /// reaches.
    pub(crate) released_reachable: u64,
    /// The blocks to release that a walk from the roots reaches.
    pub(crate) to_release_reachable: u64,
    /// The blocks to release that a walk of each slot finds.
    pub(crate) to_release_slots: u64,
    /// The first findings, at most eight. A finding of the walk of the
    /// slots has no holder.
    pub(crate) findings: Vec<MarkFinding>,
}

impl SweepCheck {
    /// The count of each finding of the check.
    pub(crate) fn count(&self) -> u64 {
        self.released_reachable
            .saturating_add(self.to_release_reachable)
            .saturating_add(self.to_release_slots)
    }

    /// The message of a failed check.
    pub(crate) fn message(&self) -> String {
        let mut message = format!(
            "compiler.md §357 rule 15: a walk from the roots reaches {} released blocks and {} blocks to release; a walk of each slot finds {} blocks to release",
            self.released_reachable, self.to_release_reachable, self.to_release_slots
        );
        for finding in &self.findings {
            let holder = finding.holder_class.map_or_else(
                || "no holder".to_owned(),
                |class| format!("class {class:#x}"),
            );
            message.push_str(&format!(
                "; block {:#x} of class {:#x}, {holder}, offset {}",
                finding.block, finding.class_id, finding.offset
            ));
        }
        message
    }

    fn find(&mut self, finding: MarkFinding) {
        if self.findings.len() < CHECK_FINDINGS {
            self.findings.push(finding);
        }
    }
}

impl Context {
    /// The live word of the Context (`compiler.md` §357 rule 1).
    #[cfg(test)]
    pub(crate) fn live_word(&self) -> u64 {
        self.mark_cycle.live_word
    }

    /// The marked word of the Context (`compiler.md` §357 rule 1). During a
    /// sweep it is the word of a block to release.
    #[cfg(test)]
    pub(crate) fn marked_word(&self) -> u64 {
        self.mark_cycle.marked_word
    }

    /// True for the state word of a block that is not released: the live
    /// word, the marked word, or the state "in the work list" (`compiler.md`
    /// §356 rule 10). A block to release is live until the sweep releases
    /// it (§357 rule 3).
    #[inline]
    pub(super) fn header_is_live(&self, state: u64) -> bool {
        state == self.mark_cycle.live_word
            || state == self.mark_cycle.marked_word
            || state == GREY_STATE
    }

    /// True for the state word of a block to release (`compiler.md` §357
    /// rule 1): a sweep is in progress, and the word is the former live
    /// word.
    #[inline]
    pub(super) fn header_to_release(&self, state: u64) -> bool {
        self.mark_cycle.sweep.active && state == self.mark_cycle.marked_word
    }

    /// True while a sweep is in progress (`compiler.md` §357 rule 4).
    pub(super) fn sweep_in_progress(&self) -> bool {
        self.mark_cycle.sweep.active
    }

    /// True once, in the collection that ends a cycle in steps
    /// (`compiler.md` §357 rule 4).
    pub(super) fn take_sweep_in_steps(&mut self) -> bool {
        std::mem::take(&mut self.mark_cycle.sweep.in_steps)
    }

    /// The slots that the sweep in progress did not read.
    pub(super) fn sweep_slots_left(&self) -> usize {
        self.mark_cycle.sweep.left
    }

    /// The end of a cycle in steps (`compiler.md` §356 rule 5 and §357 rule
    /// 4): the releases, the large blocks, and the exchange, in one pause.
    /// The small blocks are left to the sweep steps of `budget` words.
    pub(super) fn end_cycle_with_sweep_in_steps(&mut self, budget: usize) {
        let last_live = self.collection.stats.last_live_bytes_after();
        let base_and_limit = self.collection_base_and_limit();
        self.mark_cycle.sweep.in_steps = true;
        self.run_whole_collection(CollectionKind::Automatic);
        // A collection that did not run (a debug trap) leaves the flag set.
        self.mark_cycle.sweep.in_steps = false;
        if !self.sweep_in_progress() {
            return;
        }
        // compiler.md §357 rule 13: the record keeps the live bytes of the
        // last sweep that ended.
        self.collection.stats.set_last_live_bytes_after(last_live);
        // compiler.md §357 rule 11: the limit is set when the sweep ends.
        self.keep_base_and_limit(base_and_limit);
        // compiler.md §357 rule 6: the sweep steps share the requested bytes
        // that are left before the start of the next cycle, so that the
        // sweep ends before it. The sweep ends at a fraction of them: the
        // slots that it releases return early to the free lists, and the
        // reserved memory grows less.
        let steps = self
            .mark_cycle
            .sweep
            .left
            .div_ceil(self.sweep_step_slots(budget))
            .max(1);
        let left = self
            .cycle_start_threshold()
            .saturating_sub(self.collection.requested);
        self.mark_cycle.sweep.part_bytes = (left / (SWEEP_END_DIVISOR * steps)).max(MIN_PART_BYTES);
        self.place_sweep_pause();
    }

    /// True if the block at `payload` has the mark or is in the work list.
    fn block_has_mark(&self, payload: usize) -> bool {
        let block = self
            .arena_lookup_block(payload)
            .map(|(block, _)| block)
            .or_else(|| self.large.get(&payload).map(|allocation| allocation.base));
        // SAFETY: an exact allocation lookup proves that the header is
        // readable.
        block.is_some_and(|block| {
            let state = unsafe { block.cast::<u64>().read() };
            state == self.mark_cycle.marked_word || state == GREY_STATE
        })
    }

    /// `compiler.md` §357 rules 1 and 4: after the mark and the releases
    /// that read the marks, the regex values and the large blocks with no
    /// mark are released, and the Context exchanges the two words. Then a
    /// sweep is in progress from the first slot of the first chunk.
    pub(super) fn begin_sweep(&mut self) {
        // The state of a regex value whose block has no mark goes.
        let stale: Vec<usize> = self
            .regex
            .value_handles()
            .into_iter()
            .filter(|handle| !self.block_has_mark(*handle))
            .collect();
        for handle in stale {
            self.regex.remove_value(handle);
        }
        let marked = self.mark_cycle.marked_word;
        let mut freed: Vec<usize> = Vec::new();
        for (&address, allocation) in &self.large {
            // SAFETY: `base` heads an owned large allocation.
            unsafe {
                let state = (allocation.base as *const u64).read();
                debug_assert_ne!(state, GREY_STATE, "a block in the work list at the sweep");
                // A release build keeps a block in the work list.
                if state == GREY_STATE {
                    (allocation.base as *mut u64).write(marked);
                } else if state != marked {
                    freed.push(address);
                }
            }
        }
        for address in freed {
            if let Some(allocation) = self.large.remove(&address) {
                self.live_bytes_counter = self
                    .live_bytes_counter
                    .saturating_sub(allocation.payload_size);
                // SAFETY: `base`/`layout` came from `alloc_zeroed` in
                // `arena_alloc_large`; the record was just removed, so this
                // frees it exactly once.
                unsafe { dealloc(allocation.base, allocation.layout) };
                #[cfg(test)]
                self.stats
                    .large
                    .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        // compiler.md §357 rule 15: mode 3 and a test keep the released
        // payloads for the check at the completion.
        let checked = self.sweep_is_checked();
        // compiler.md §357 rule 1: the exchange. A new block gets the live
        // word.
        let cycle = &mut self.mark_cycle;
        std::mem::swap(&mut cycle.live_word, &mut cycle.marked_word);
        cycle.alloc_state = cycle.live_word;
        let limits: Vec<usize> = self.chunks.iter().map(|chunk| chunk.bump).collect();
        let total = limits.iter().sum();
        let sweep = &mut cycle.sweep;
        sweep.active = true;
        sweep.limits = limits;
        sweep.chunk = 0;
        sweep.slot = 0;
        sweep.total = total;
        sweep.left = total;
        sweep.base_at_end = self.live_bytes_counter;
        sweep.released_bytes = 0;
        sweep.late = false;
        sweep.released = checked.then(AddressSet::default);
    }

    /// `compiler.md` §357 rule 5: reads at most `max` slots from the
    /// position of the sweep, and puts each block to release on the free
    /// list of its class. It writes nothing for a live block. Returns the
    /// slots that it read. If it read the last slot, the sweep completes.
    pub(super) fn sweep_slots(&mut self, max: usize) -> usize {
        let release = self.mark_cycle.marked_word;
        let live = self.mark_cycle.live_word;
        let mut swept = 0usize;
        while swept < max {
            let ci = self.mark_cycle.sweep.chunk;
            let Some(&limit) = self.mark_cycle.sweep.limits.get(ci) else {
                break;
            };
            let start = self.mark_cycle.sweep.slot;
            let end = limit.min(start.saturating_add(max - swept));
            let (base, block_size, class) = {
                let chunk = &self.chunks[ci];
                (chunk.base, chunk.block_size, chunk.class)
            };
            let mut free_head = self.free_heads[class];
            let mut retired_bytes = 0usize;
            for bi in start..end {
                // SAFETY: `bi < limit <= bump`, so the block is inside the
                // owned chunk; the state word and the first two words of the
                // payload (the free-list link and the chunk index) stay
                // inside it.
                unsafe {
                    let block = base.add(bi * block_size);
                    let state = (block as *const u64).read();
                    // compiler.md §356 rule 10: the end emptied the work
                    // list, so no block is in it.
                    debug_assert_ne!(state, GREY_STATE, "a block in the work list at the sweep");
                    if state == release {
                        (block as *mut u64).write(DEAD_STATE);
                        let payload = block.add(HEADER_SIZE);
                        (payload as *mut usize).write(free_head);
                        // compiler.md §346 rule 1a: the chunk index for the
                        // free-list pop.
                        (payload as *mut usize).add(1).write(ci);
                        free_head = payload as usize;
                        retired_bytes += block_size - HEADER_SIZE;
                        if let Some(released) = self.mark_cycle.sweep.released.as_mut() {
                            released.insert(payload as usize);
                        }
                    } else if state == GREY_STATE {
                        // A release build keeps a block in the work list.
                        (block as *mut u64).write(live);
                    }
                }
            }
            self.free_heads[class] = free_head;
            self.live_bytes_counter = self.live_bytes_counter.saturating_sub(retired_bytes);
            let sweep = &mut self.mark_cycle.sweep;
            sweep.released_bytes = sweep.released_bytes.saturating_add(retired_bytes);
            let read = end - start;
            swept += read;
            sweep.left = sweep.left.saturating_sub(read);
            if end >= limit {
                sweep.chunk += 1;
                sweep.slot = 0;
            } else {
                sweep.slot = end;
            }
        }
        self.collection.stats.add_pause_slots(swept);
        if self.mark_cycle.sweep.chunk >= self.mark_cycle.sweep.limits.len() {
            self.complete_sweep();
        }
        swept
    }

    /// The sweep read each slot (`compiler.md` §357 rules 11 and 15).
    fn complete_sweep(&mut self) {
        if !self.mark_cycle.sweep.active {
            return;
        }
        let sweep = &mut self.mark_cycle.sweep;
        sweep.active = false;
        sweep.limits = Vec::new();
        sweep.left = 0;
        let base = sweep.base_at_end.saturating_sub(sweep.released_bytes);
        if self.sweep_is_checked() {
            let check = self.sweep_check();
            // compiler.md §357 rule 15: mode 3 stops the process with a
            // message if the check fails.
            if self.collection.settings.mode == CollectionMode::StepStress && check.count() != 0 {
                eprintln!("{}", check.message());
                std::process::abort();
            }
            if self.mark_cycle.sweep.record_checks {
                self.mark_cycle.sweep.checks.push(check);
            }
        }
        self.mark_cycle.sweep.released = None;
        self.after_sweep_in_steps(base);
    }

    /// True when the completion of a sweep runs the check of `compiler.md`
    /// §357 rule 15: in mode 3, and in a test that records the checks.
    fn sweep_is_checked(&self) -> bool {
        self.mark_cycle.sweep.record_checks
            || self.collection.settings.mode == CollectionMode::StepStress
    }

    /// `compiler.md` §357 rules 9 and 10: a collection completes a sweep in
    /// progress, in its own pause and with no budget.
    pub(super) fn complete_sweep_now(&mut self) {
        if self.sweep_in_progress() {
            self.sweep_slots(usize::MAX);
        }
    }

    /// The slots of one sweep step of `budget` words (`compiler.md` §357
    /// rules 5 and 15). A budget of 0 has no limit.
    fn sweep_step_slots(&self, budget: usize) -> usize {
        if budget == 0 {
            return usize::MAX;
        }
        let slots = budget.saturating_mul(SLOTS_PER_WORD);
        if self.collection.settings.mode == CollectionMode::StepStress {
            return slots.max(self.mark_cycle.sweep.total.div_ceil(STRESS_SWEEP_STEPS));
        }
        slots
    }

    /// One sweep step of `budget` words (`compiler.md` §357 rules 5, 9, and
    /// 12). A budget of 0 completes the sweep. Returns true if the sweep is
    /// in progress after the step.
    pub(super) fn sweep_pause(&mut self, budget: usize) -> bool {
        let started = self.collection.stats.start();
        self.note_late_sweep();
        let slots = self.sweep_step_slots(budget);
        self.sweep_slots(slots);
        self.collection.stats.finish_sweep_step(started);
        let active = self.sweep_in_progress();
        if active {
            self.place_sweep_pause();
        }
        active
    }

    /// `compiler.md` §357 rule 9: in mode 1, a sweep is late when the
    /// requested bytes exceed the start of the next cycle. The record
    /// counts it one time.
    fn note_late_sweep(&mut self) {
        if self.mark_cycle.sweep.late
            || self.collection.settings.mode != CollectionMode::Automatic
            || self.collection.requested <= self.cycle_start_threshold()
        {
            return;
        }
        self.mark_cycle.sweep.late = true;
        self.collection.stats.count_late_sweep();
    }

    /// `compiler.md` §357 rules 6 and 9: in mode 1 the allocation sets the
    /// pending flag after each part of the requested bytes. A late sweep
    /// keeps the flag set, so that each poll and each outermost exit runs
    /// one sweep step. In mode 2 and mode 3 the flag stays set; in mode 0
    /// the host runs each step.
    fn place_sweep_pause(&mut self) {
        match self.collection.settings.mode {
            CollectionMode::Automatic => {
                let threshold = self.cycle_start_threshold();
                if self.mark_cycle.sweep.late || self.collection.requested > threshold {
                    self.collection_pending = 1;
                } else {
                    self.collection.pending_at = self
                        .collection
                        .requested
                        .saturating_add(self.mark_cycle.sweep.part_bytes)
                        .min(threshold);
                    self.collection_pending = 0;
                }
            }
            CollectionMode::Off => self.collection_pending = 0,
            CollectionMode::EachPoll | CollectionMode::StepStress => self.collection_pending = 1,
        }
    }

    /// `compiler.md` §357 rule 8: the program released the block at
    /// `payload` of `bytes` payload bytes. A block to release counts in the
    /// bytes of the sweep (rule 11).
    pub(super) fn note_sweep_release(&mut self, payload: usize, bytes: usize, to_release: bool) {
        let sweep = &mut self.mark_cycle.sweep;
        if to_release {
            sweep.released_bytes = sweep.released_bytes.saturating_add(bytes);
        }
        if let Some(released) = sweep.released.as_mut() {
            released.remove(&payload);
        }
    }

    /// The check of a sweep (`compiler.md` §357 rule 15), for a Context
    /// with no sweep and no cycle in progress. A walk from the roots finds
    /// no block that the last sweep released and no block to release; a
    /// walk of each slot finds no block to release. The walk reads a
    /// payload as the mark does, so a number that equals the address of a
    /// block can fail the check. A released block counts only while the
    /// Context keeps the payloads that the sweep released: when the check
    /// runs at the completion of the sweep.
    pub(crate) fn sweep_check(&self) -> SweepCheck {
        let release = self.mark_cycle.marked_word;
        let released = self.mark_cycle.sweep.released.as_ref();
        let tasks = self.completed_tasks();
        let mut roots = Vec::new();
        self.push_roots(&mut roots, &mut None, &tasks);
        let mut work: Vec<(usize, Option<u32>, usize)> = roots
            .into_iter()
            .map(|address| (address, None, 0))
            .collect();
        let mut seen = AddressSet::default();
        let mut check = SweepCheck::default();
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
            if !seen.insert(address) {
                continue;
            }
            // SAFETY: an exact allocation lookup proves that the header is
            // readable.
            let (state, class_id) = unsafe { (block.cast::<u64>().read(), header_class_id(block)) };
            let finding = MarkFinding {
                block: address,
                class_id,
                holder_class,
                offset,
            };
            if state == DEAD_STATE {
                if released.is_some_and(|released| released.contains(&address)) {
                    check.released_reachable += 1;
                    check.find(finding);
                }
                continue;
            }
            if state == release {
                check.to_release_reachable += 1;
                check.find(finding);
            } else if !self.header_is_live(state) {
                continue;
            }
            if class_id == CLASS_GENERATOR && tasks.contains(address) {
                let mut words = Vec::new();
                self.push_frame_completion(address, &mut words);
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
        for chunk in &self.chunks {
            for bi in 0..chunk.bump {
                // SAFETY: `bi < bump`, so the header lies in the owned chunk.
                let block = unsafe { chunk.base.add(bi * chunk.block_size) };
                // SAFETY: as above.
                let (state, class_id) =
                    unsafe { (block.cast::<u64>().read(), header_class_id(block)) };
                if state == release {
                    check.to_release_slots += 1;
                    check.find(MarkFinding {
                        block: block.wrapping_add(HEADER_SIZE) as usize,
                        class_id,
                        holder_class: None,
                        offset: 0,
                    });
                }
            }
        }
        for (&payload, allocation) in &self.large {
            // SAFETY: each large record owns a complete header.
            let (state, class_id) = unsafe {
                (
                    allocation.base.cast::<u64>().read(),
                    header_class_id(allocation.base),
                )
            };
            if state == release {
                check.to_release_slots += 1;
                check.find(MarkFinding {
                    block: payload,
                    class_id,
                    holder_class: None,
                    offset: 0,
                });
            }
        }
        check
    }

    /// Makes the completion of each sweep keep the result of the check of
    /// `compiler.md` §357 rule 15.
    #[cfg(test)]
    pub(crate) fn record_sweep_checks(&mut self) {
        self.mark_cycle.sweep.record_checks = true;
    }

    /// The results of the checks that the completions of the sweeps kept.
    #[cfg(test)]
    pub(crate) fn sweep_checks(&self) -> &[SweepCheck] {
        &self.mark_cycle.sweep.checks
    }

    /// The position of the sweep in progress: the chunk and the slot.
    #[cfg(test)]
    pub(crate) fn sweep_position(&self) -> (usize, usize) {
        (self.mark_cycle.sweep.chunk, self.mark_cycle.sweep.slot)
    }
}
