use super::*;

impl Context {
    /// Iterates live classed arena blocks as `(header, block_size)`.
    pub(super) fn live_blocks(&self) -> impl Iterator<Item = (*mut u8, usize)> + '_ {
        self.chunks.iter().flat_map(|chunk| {
            (0..chunk.bump).filter_map(move |bi| {
                // SAFETY: `bi < bump`, so the header lies in the owned chunk.
                let base = unsafe { chunk.base.add(bi * chunk.block_size) };
                // SAFETY: `base` points to a complete initialized header.
                // compiler.md §356: a block that a mark in progress reached,
                // or that is in its work list, is live.
                matches!(
                    unsafe { base.cast::<u64>().read() },
                    LIVE_STATE | MARK_STATE | GREY_STATE
                )
                .then_some((base, chunk.block_size))
            })
        })
    }

    /// Number of live allocations (test/inspection aid). Ship tier: a
    /// chunk walk (live blocks below each watermark) plus the large
    /// records.
    #[must_use]
    pub fn live_count(&self) -> usize {
        if self.uses_ship_arena() {
            return self.large.len() + self.live_blocks().count();
        }
        self.allocations.len()
    }

    /// Payload capacity in live allocations.
    ///
    /// The development tier reports exact requested payload sizes. The
    /// ship tier reports size-class payload capacity for arena blocks and
    /// exact payload size for large allocations.
    ///
    /// §113.2 rule 4: this is a counter the runtime maintains at every
    /// change of the live set, in both memory modes. A host reads it once
    /// per frame, so it must not walk the live set. Collection compares
    /// it against the walk.
    #[must_use]
    pub fn live_bytes(&self) -> usize {
        self.live_bytes_counter
    }

    /// The same quantity as [`Context::live_bytes`], derived instead by a
    /// walk over the live set.
    ///
    /// The two derivations are independent, so collection and the unit
    /// tests compare them (§113.2 rule 4).
    #[cfg(any(test, debug_assertions))]
    pub(super) fn live_bytes_by_walk(&self) -> usize {
        if self.uses_ship_arena() {
            let mut bytes = self
                .large
                .values()
                .fold(0usize, |sum, a| sum.saturating_add(a.payload_size));
            for (_, block_size) in self.live_blocks() {
                bytes = bytes.saturating_add(block_size - HEADER_SIZE);
            }
            return bytes;
        }
        self.allocations
            .values()
            .fold(0usize, |sum, a| sum.saturating_add(a.payload_size))
    }

    /// Bytes currently reserved from the system for Context allocations.
    ///
    /// Exact-size allocations include only live layouts unless freed-handle
    /// diagnostics retain dead layouts. Ship-tier chunks remain reserved,
    /// with the request record of each chunk: 2 bytes for each block
    /// (`compiler.md` §346 rule 2c). A deleted large allocation is returned
    /// to the system immediately.
    #[must_use]
    pub fn reserved_bytes(&self) -> usize {
        if self.uses_ship_arena() {
            let chunk_bytes = self.chunks.iter().fold(0usize, |sum, c| {
                sum.saturating_add(c.layout.size())
                    .saturating_add(std::mem::size_of_val(&*c.requests))
            });
            return self
                .large
                .values()
                .fold(chunk_bytes, |sum, a| sum.saturating_add(a.layout.size()));
        }
        let live_layout_bytes = self
            .allocations
            .values()
            .fold(0usize, |sum, a| sum.saturating_add(a.layout.size()));
        live_layout_bytes.saturating_add(self.retained_bytes)
    }

    #[cfg(test)]
    pub(crate) fn test_arena_stats(&self) -> std::sync::Arc<ArenaStats> {
        std::sync::Arc::clone(&self.stats)
    }

    /// Visits every live allocation and returns the number visited.
    ///
    /// The iteration order is unspecified. A null visitor performs no
    /// callbacks and returns zero.
    ///
    /// # Safety
    ///
    /// `visitor`, when present, must be callable with `userdata` for the
    /// duration of this call.
    pub unsafe fn visit_live_allocations(
        &self,
        visitor: Option<AllocationVisitor>,
        userdata: *mut c_void,
    ) -> u64 {
        let Some(visitor) = visitor else {
            return 0;
        };
        let mut count = 0u64;
        if self.uses_ship_arena() {
            for (base, block_size) in self.live_blocks() {
                // SAFETY: `live_blocks` returns initialized live headers.
                let (class_id, pos_id) = unsafe { (header_class_id(base), header_pos_id(base)) };
                let class_id = reported_class_id(class_id);
                // SAFETY: the host supplied `visitor` and its userdata.
                unsafe {
                    visitor(
                        userdata,
                        class_id,
                        pos_id,
                        (block_size - HEADER_SIZE) as u64,
                    )
                };
                count += 1;
            }
            for allocation in self.large.values() {
                // Every retained large record is live. The complete
                // header was initialized by `arena_alloc_large`.
                let (class_id, pos_id) = unsafe {
                    (
                        header_class_id(allocation.base),
                        header_pos_id(allocation.base),
                    )
                };
                let class_id = reported_class_id(class_id);
                // SAFETY: the host supplied `visitor` and its userdata.
                unsafe { visitor(userdata, class_id, pos_id, allocation.payload_size as u64) };
                count += 1;
            }
            return count;
        }

        for allocation in self.allocations.values() {
            // SAFETY: a live retained allocation owns a fully initialized
            // header for the lifetime of the Context.
            let (class_id, pos_id) = unsafe {
                (
                    header_class_id(allocation.base),
                    header_pos_id(allocation.base),
                )
            };
            let class_id = reported_class_id(class_id);
            // SAFETY: the host supplied `visitor` and its userdata.
            unsafe { visitor(userdata, class_id, pos_id, allocation.payload_size as u64) };
            count += 1;
        }
        count
    }

    // ----- roots and collection -----

    /// Registers a permanent root range: `words` consecutive 8-byte
    /// slots at `base`, conservatively scanned for managed handles.
    /// One word for a scalar managed global; several for a global
    /// aggregate (e.g. a `FixedArray` of references) whose interior
    /// holds handles.
    ///
    /// # Safety
    /// The range contains `words` readable 8-byte slots until the Context drops.
    /// See compiler.md section 263.
    pub unsafe fn root_add(&mut self, base: usize, words: usize) {
        self.roots.push((base, words));
    }

    /// Pushes a shadow frame: `slots` consecutive 8-byte slots at
    /// `base`, each holding a managed local (or null).
    ///
    /// # Safety
    /// The range contains `slots` readable 8-byte slots until the matching shadow pop.
    /// See compiler.md section 263.
    pub unsafe fn shadow_push(&mut self, base: usize, slots: usize) {
        self.shadow.push((base, slots));
    }

    /// Pops the most recent shadow frame.
    pub fn shadow_pop(&mut self) {
        self.shadow.pop();
    }

    /// The explicit collection (Q7): frees every allocation not reachable
    /// from the registered roots. It reports the failure of an unobserved
    /// task that it finds or that an automatic collection recorded
    /// (`compiler.md` §342 rule 13).
    pub fn collect(&mut self) {
        self.collect_explicit_own();
    }

    /// One full collection of the roots that the Context holds. The check
    /// walks of a debug build run outside it (`compiler.md` §353 rule 4).
    pub(super) fn collect_full(&mut self) {
        self.collect_with_trace(mark_trace_target());
    }

    /// Reports every live dynamic array whose unused data tail contains a
    /// nonzero byte.
    #[must_use]
    pub fn array_tail_violations(&self) -> Vec<ArrayTailViolation> {
        let mut violations = Vec::new();
        if !self.uses_ship_arena() {
            for (&handle, allocation) in &self.allocations {
                if is_array_class(allocation.class_id) {
                    // SAFETY: exact live-map membership and the class id
                    // prove that `handle` is a live array header.
                    unsafe { self.record_array_tail_violation(handle, &mut violations) };
                }
            }
            violations.sort_unstable_by_key(|violation| violation.handle);
            return violations;
        }

        for chunk in &self.chunks {
            for block_index in 0..chunk.bump {
                // SAFETY: the index is below the chunk's bump watermark, so
                // the state and class-id reads stay inside the owned block.
                let (state, class_id, handle) = unsafe {
                    let block = chunk.base.add(block_index * chunk.block_size);
                    (
                        block.cast::<u64>().read(),
                        header_class_id(block),
                        block.add(HEADER_SIZE) as usize,
                    )
                };
                if matches!(state, LIVE_STATE | MARK_STATE | GREY_STATE) && is_array_class(class_id)
                {
                    // SAFETY: the live grid state and class id prove that
                    // `handle` is a live array header.
                    unsafe { self.record_array_tail_violation(handle, &mut violations) };
                }
            }
        }
        for (&handle, allocation) in &self.large {
            // SAFETY: each large record owns a complete initialized header.
            let class_id = unsafe { header_class_id(allocation.base) };
            if is_array_class(class_id) {
                // SAFETY: exact large-map membership and the class id prove
                // that `handle` is a live array header.
                unsafe { self.record_array_tail_violation(handle, &mut violations) };
            }
        }
        violations.sort_unstable_by_key(|violation| violation.handle);
        violations
    }

    /// Adds one violation for a live array header when its tail is nonzero.
    ///
    /// # Safety
    ///
    /// `handle` must identify a live array header payload owned by this
    /// context.
    pub(super) unsafe fn record_array_tail_violation(
        &self,
        handle: usize,
        violations: &mut Vec<ArrayTailViolation>,
    ) {
        // SAFETY: the caller proves that the payload is a live array header.
        let header = unsafe { &*(handle as *const ArrayHeader) };
        if header.data.is_null() {
            return;
        }
        let len = match usize::try_from(header.len) {
            Ok(len) => len,
            Err(_) => {
                debug_assert!(false, "array len does not fit usize: {}", header.len);
                return;
            }
        };
        let cap = match usize::try_from(header.cap) {
            Ok(cap) => cap,
            Err(_) => {
                debug_assert!(false, "array cap does not fit usize: {}", header.cap);
                return;
            }
        };
        let elem_size = match usize::try_from(header.elem_size) {
            Ok(elem_size) => elem_size,
            Err(_) => {
                debug_assert!(
                    false,
                    "array elem_size does not fit usize: {}",
                    header.elem_size
                );
                return;
            }
        };
        debug_assert!(len <= cap, "array len exceeds cap: len {len}, cap {cap}");
        if len > cap {
            return;
        }
        let Some(tail_start) = len.checked_mul(elem_size) else {
            debug_assert!(false, "array len * elem_size does not fit usize");
            return;
        };
        let data = header.data as usize;
        let Some(payload_size) = self.live_payload_size(data) else {
            violations.push(ArrayTailViolation {
                handle,
                len: header.len,
                offset: 0,
            });
            return;
        };
        if tail_start > payload_size {
            violations.push(ArrayTailViolation {
                handle,
                len: header.len,
                offset: 0,
            });
            return;
        }
        if tail_start == payload_size {
            return;
        }
        // SAFETY: the tier-specific live lookup proves that `data` owns
        // `payload_size` readable bytes. `tail_start` is within that payload.
        let tail = unsafe {
            std::slice::from_raw_parts(header.data.add(tail_start), payload_size - tail_start)
        };
        if let Some(relative_offset) = tail.iter().position(|byte| *byte != 0) {
            violations.push(ArrayTailViolation {
                handle,
                len: header.len,
                offset: tail_start + relative_offset,
            });
        }
    }

    /// Returns the complete payload size for one live allocation in the
    /// active allocator tier.
    pub(super) fn live_payload_size(&self, payload: usize) -> Option<usize> {
        if !self.uses_ship_arena() {
            return self
                .allocations
                .get(&payload)
                .map(|allocation| allocation.payload_size);
        }
        if let Some((block, class)) = self.arena_lookup_block(payload) {
            // SAFETY: arena membership proves that the state word is inside
            // an owned block.
            let live = matches!(
                unsafe { block.cast::<u64>().read() },
                LIVE_STATE | MARK_STATE | GREY_STATE
            );
            return live.then_some((SMALLEST_BLOCK << class) - HEADER_SIZE);
        }
        self.large
            .get(&payload)
            .map(|allocation| allocation.payload_size)
    }

    pub(super) fn push_mark_word(
        &self,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
        address: usize,
        source: MarkSource,
    ) {
        work.push(address);
        let Some(tracer) = tracer else {
            return;
        };
        let class_id = if self.uses_ship_arena() {
            if let Some((block, _)) = self.arena_lookup_block(address) {
                // SAFETY: arena membership gives an owned block with a
                // complete initialized header.
                Some(unsafe { header_class_id(block) })
            } else {
                self.large.get(&address).map(|allocation| {
                    // SAFETY: exact large-map membership gives an owned
                    // allocation with a complete initialized header.
                    unsafe { header_class_id(allocation.base) }
                })
            }
        } else {
            self.allocations
                .get(&address)
                .map(|allocation| allocation.class_id)
        };
        if let Some(class_id) = class_id {
            tracer.record(address, class_id, source);
        }
    }

    pub(super) fn push_root_set(
        &self,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
        set: &'static str,
        values: impl Iterator<Item = (usize, usize, usize)>,
    ) {
        for (index, word, address) in values {
            self.push_mark_word(work, tracer, address, MarkSource::Root { set, index, word });
        }
    }

    /// Pushes each word of a payload to the work list, and returns the count
    /// of the words that it pushed (`compiler.md` §353 rule 5).
    pub(super) fn scan_payload(
        &self,
        payload: *const u8,
        size: usize,
        class_id: u32,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
    ) -> usize {
        // compiler.md §344 rule 1: the storage of an array whose elements
        // hold no handle is marked as reached and is not read.
        if class_holds_no_handle(class_id) {
            return 0;
        }
        #[cfg(test)]
        self.stats
            .words_read
            .fetch_add(size / 8, std::sync::atomic::Ordering::Relaxed);
        for word in 0..size / 8 {
            // SAFETY: the caller supplies an owned payload of `size` bytes.
            let address = unsafe { payload.add(word * 8).cast::<usize>().read_unaligned() };
            self.push_mark_word(
                work,
                tracer,
                address,
                MarkSource::Payload {
                    class_id,
                    address: payload as usize,
                    word,
                },
            );
        }
        size / 8
    }

    fn prepare_collection(
        &mut self,
        trace_target: Option<MarkTraceTarget>,
        late: &[usize],
    ) -> CollectionReleases {
        let mut work: Vec<usize> = Vec::new();
        let mut tracer = trace_target.map(MarkTracer::new);
        let tasks = self.completed_tasks();
        // compiler.md §356 rules 1, 5, and 8: the end of a cycle reads the
        // roots again and empties the work list of the steps, which counted
        // those words. An end whose mark had the budget read them already.
        let carried = if self.take_end_marked() {
            0
        } else {
            self.push_roots(&mut work, &mut tracer, &tasks);
            self.take_cycle_work(&mut work)
        };
        self.drain_mark(&mut work, &mut tracer, &tasks);
        self.collection.stats.remove_mark_words(carried);
        self.close_cycle_mark();
        self.root_tasks_with_outside_holders(&tasks, &mut work, &mut tracer);
        let (handles, storage) = self.unreachable_releases();
        let ending_tasks = self.mark_ending_task_failures(&tasks, late, &mut work, &mut tracer);
        // compiler.md §356 rule 20: after the mark, before the sweep.
        self.check_at_cycle_end();
        CollectionReleases {
            handles,
            storage,
            ending_tasks,
            trace: tracer.map_or_else(Vec::new, |tracer| tracer.records),
        }
    }

    /// Pushes each root of the Context to the work list. The mark of a
    /// collection, the start of a cycle, and the check of `compiler.md`
    /// §356 rule 20 read the same roots.
    pub(super) fn push_roots(
        &self,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
        tasks: &CompletedTasks,
    ) {
        let roots = self
            .roots
            .iter()
            .enumerate()
            .flat_map(|(index, &(base, words))| {
                (0..words).map(move |word| {
                    // SAFETY: generated code registered this live root range.
                    let address = unsafe { ((base + word * 8) as *const usize).read_unaligned() };
                    (index, word, address)
                })
            });
        self.push_root_set(work, tracer, "roots", roots);
        let borrowed = self
            .boundary_borrows
            .iter()
            .filter_map(|record| match record {
                BoundaryBorrow::Value { value, .. } => Some(*value),
                BoundaryBorrow::Scope { .. } => None,
            })
            .enumerate()
            .map(|(index, value)| (index, 0, value));
        self.push_root_set(work, tracer, "native_borrows", borrowed);
        let shadow = self
            .shadow
            .iter()
            .enumerate()
            .flat_map(|(index, &(base, slots))| {
                (0..slots).map(move |word| {
                    // SAFETY: generated code registered this live shadow range.
                    let address = unsafe { ((base + word * 8) as *const usize).read_unaligned() };
                    (index, word, address)
                })
            });
        self.push_root_set(work, tracer, "shadow", shadow);
        // compiler.md §115.2 rule 4: a pending exception is a root.
        let pending = self
            .pending_exception
            .iter()
            .map(|pending| (0, 0, pending.object));
        self.push_root_set(work, tracer, "pending_exception", pending);
        // compiler.md §115.5 rule 8: the park stack is a collection root.
        let parked = self
            .parked_exceptions
            .iter()
            .map(|parked| (0, 0, parked.object));
        self.push_root_set(work, tracer, "parked_exceptions", parked);
        // compiler.md §342 rule 12: a recorded failure keeps its Error object.
        let recorded = self.recorded_failure_objects().map(|object| (0, 0, object));
        self.push_root_set(work, tracer, "recorded_failures", recorded);
        // §94.2: every scheduler state is a collection root. Ready jobs,
        // parked frames, and blocked continuations are named separately, so
        // the enumeration shows each state rather than relying on the
        // registration map to cover them all.
        let async_ready = self
            .async_ready
            .iter()
            .enumerate()
            .map(|(index, job)| (index, 0, job.handle() as usize));
        self.push_root_set(work, tracer, "async_ready", async_ready);
        let async_parked = self
            .async_parked
            .iter()
            .enumerate()
            .map(|(index, frame)| (index, 0, *frame as usize));
        self.push_root_set(work, tracer, "async_parked", async_parked);
        let async_stopped = self
            .async_stopped
            .iter()
            .enumerate()
            .map(|(index, frame)| (index, 0, *frame as usize));
        self.push_root_set(work, tracer, "async_stopped", async_stopped);
        let async_blocked = self
            .async_frames
            .values()
            .flat_map(|meta| meta.waiters.iter())
            .copied()
            .enumerate()
            .map(|(index, job)| (index, 0, job.handle() as usize));
        self.push_root_set(work, tracer, "async_blocked", async_blocked);
        let active_async_frames = self
            .active_async_frames
            .iter()
            .copied()
            .enumerate()
            .map(|(index, address)| (index, 0, address));
        self.push_root_set(work, tracer, "active_async_frames", active_async_frames);
        // compiler.md §348 rules 1 and 5: each registered frame is a root,
        // except a completed task. The mark reads a completed task when it
        // reaches it (rule 2), and the step after the mark keeps a completed
        // task that an outside holder owns (rule 3). A cycle of unfinished
        // tasks stays a root and leaks (§70.3 rule 6).
        let async_frames = self
            .async_frames
            .keys()
            .copied()
            .filter(|frame| !tasks.contains(*frame))
            .enumerate()
            .map(|(index, address)| (index, 0, address));
        self.push_root_set(work, tracer, "async_frames", async_frames);
        let group_roots = self.task_group_roots();
        self.push_root_set(work, tracer, "task_groups", group_roots.into_iter());
        let aggregate_roots = self.async_aggregate_roots();
        self.push_root_set(work, tracer, "async_aggregate", aggregate_roots.into_iter());
        let completions = self
            .async_frames
            .iter()
            .filter(|(frame, _)| !tasks.contains(**frame))
            .filter_map(|(_, meta)| meta.completion.as_ref()?.value())
            .enumerate()
            .flat_map(|(index, completion)| {
                completion
                    .chunks_exact(core::mem::size_of::<usize>())
                    .enumerate()
                    .map(move |(word_index, word)| {
                        // SAFETY: `word` is exactly one native word of Context-owned
                        // completion storage; unaligned reads preserve aggregate
                        // layouts while exposing any managed handle to the marker.
                        let address = unsafe { word.as_ptr().cast::<usize>().read_unaligned() };
                        (index, word_index, address)
                    })
            });
        self.push_root_set(work, tracer, "completion", completions);
        // compiler.md §348 rules 1 and 2: the Error object of each frame that
        // is not a completed task is a root. A completed task gives its Error
        // object when the mark reaches it.
        let completion_exceptions = self
            .async_frames
            .iter()
            .filter(|(frame, _)| !tasks.contains(**frame))
            .filter_map(|(_, meta)| meta.completion.as_ref()?.exception_object())
            .enumerate()
            .map(|(index, address)| (index, 0, address));
        self.push_root_set(work, tracer, "completion_exception", completion_exceptions);
        let interned = self
            .interned
            .values()
            .copied()
            .enumerate()
            .map(|(index, address)| (index, 0, address));
        self.push_root_set(work, tracer, "interned", interned);
        let astral = self
            .astral_code_points
            .values()
            .copied()
            .enumerate()
            .map(|(index, address)| (index, 0, address));
        self.push_root_set(work, tracer, "astral_code_points", astral);
        let mut callbacks = Vec::new();
        for (index, binding) in self.callbacks.iter().enumerate() {
            for (word, userdata) in [binding.userdata1, binding.userdata2]
                .into_iter()
                .enumerate()
            {
                let address = userdata as usize;
                if !userdata.is_null() && self.is_live(address) {
                    callbacks.push((index, word, address));
                }
            }
        }
        self.push_root_set(work, tracer, "callbacks", callbacks.into_iter());
        // §111 rule 8: the live set of registrations is the second root
        // source of registered userdata.
        let registrations = self.registration_roots();
        self.push_root_set(work, tracer, "registrations", registrations.into_iter());
    }

    /// Marks each allocation that the work list reaches, in the mark of the
    /// active allocator tier. A completed task that the mark reaches gives
    /// its completion to the work list (`compiler.md` §348 rule 2).
    ///
    /// `compiler.md` §353 rule 5: each word in the work list is tested once
    /// as a possible reference. So the words that the drain tests are the
    /// words in the list at entry, plus the words that each marked
    /// allocation pushes. The count is added once for each allocation, from
    /// the payload size that the scan already has.
    pub(super) fn drain_mark(
        &mut self,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
        tasks: &CompletedTasks,
    ) {
        let entry = work.len();
        let pushed = if self.uses_ship_arena() {
            self.arena_mark(work, tracer, tasks)
        } else {
            self.dev_mark(work, tracer, tasks)
        };
        self.collection
            .stats
            .add_mark_words(entry.saturating_add(pushed));
    }

    /// The mark of the dev kind. Returns the words that the marked
    /// allocations pushed (`compiler.md` §353 rule 5).
    fn dev_mark(
        &mut self,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
        tasks: &CompletedTasks,
    ) -> usize {
        let mut pushed = 0usize;
        while let Some(addr) = work.pop() {
            let Some((class_id, words)) = self.allocations.get_mut(&addr).and_then(|allocation| {
                if allocation.marked {
                    return None;
                }
                allocation.marked = true;
                Some((allocation.class_id, allocation.payload_size / 8))
            }) else {
                continue;
            };
            if class_id == CLASS_GENERATOR {
                pushed += self.push_task_completion(addr, tasks, work, tracer);
            }
            pushed += self.scan_payload(addr as *const u8, words * 8, class_id, work, tracer);
        }
        pushed
    }

    /// Exact-size allocator sweep: extract unreachable records from the
    /// only map this phase walks and reset marked survivors in the same
    /// pass.
    ///
    /// The retained-dead address index and ownership queue are deliberately
    /// never traversed here.
    pub(super) fn sweep_dev_allocations(&mut self, retiring: usize) {
        if !self.freed_handle_diagnostics {
            let live_bytes_counter = &mut self.live_bytes_counter;
            for (_, allocation) in self.allocations.extract_if(|_, allocation| {
                if allocation.marked {
                    allocation.marked = false;
                    false
                } else {
                    true
                }
            }) {
                *live_bytes_counter = live_bytes_counter.saturating_sub(allocation.payload_size);
                // SAFETY: this allocation was live at sweep entry; its
                // record was just removed, so this frees it exactly once.
                unsafe { dealloc(allocation.base, allocation.layout) };
            }
            return;
        }

        let min_payload_bytes = self.freed_handle_diagnostics_min_payload_bytes;
        let max_retained_bytes = self.freed_handle_diagnostics_max_retained_bytes;
        // Preserve the one-shot reserve for the effectively unbounded mode.
        // A finite budget instead lets both structures grow only to the
        // bounded retained set, rather than reserving for an arbitrarily
        // large unreachable burst that will mostly be evicted.
        if max_retained_bytes == usize::MAX {
            let retaining = if min_payload_bytes == 0 {
                retiring
            } else {
                self.allocations
                    .values()
                    .filter(|allocation| {
                        !allocation.marked && allocation.payload_size >= min_payload_bytes
                    })
                    .count()
            };
            self.dead_allocations.reserve(retaining);
            self.retained_allocations.reserve(retaining);
        }
        let dead_allocations = &mut self.dead_allocations;
        let retained_allocations = &mut self.retained_allocations;
        let retained_bytes = &mut self.retained_bytes;
        let live_bytes_counter = &mut self.live_bytes_counter;
        // `extract_if` retains the live map's bucket storage. Later bursts
        // reuse it; accumulated deletion tombstones can eventually force one
        // bounded rebuild, but dead-count growth does not repeat peak
        // rehashes.
        for (addr, allocation) in self.allocations.extract_if(|_, allocation| {
            if allocation.marked {
                allocation.marked = false;
                false
            } else {
                true
            }
        }) {
            *live_bytes_counter = live_bytes_counter.saturating_sub(allocation.payload_size);
            if allocation.payload_size < min_payload_bytes {
                // SAFETY: this allocation was live at sweep entry; its
                // record was just removed, so this frees it exactly once.
                unsafe { dealloc(allocation.base, allocation.layout) };
                continue;
            }
            Self::retain_or_release_removed_allocation(
                dead_allocations,
                retained_allocations,
                retained_bytes,
                max_retained_bytes,
                addr,
                allocation,
            );
        }
    }

    /// Drops per-handle RegExp state after the ordinary allocation sweep.
    ///
    /// Compiled patterns remain in the Context-lifetime cache; only state
    /// keyed by handles that collection just retired is removed.
    pub(super) fn sweep_regex_values(&mut self) {
        let stale: Vec<usize> = self
            .regex
            .value_handles()
            .into_iter()
            .filter(|handle| !self.is_live(*handle))
            .collect();
        for handle in stale {
            self.regex.remove_value(handle);
        }
    }

    /// Ship-tier mark phase (§8.1b): drains the conservative work list.
    /// A word is treated as a managed payload only under the exact
    /// membership test ([`Context::arena_lookup_block`] plus a live
    /// header, or an exact large-record match); a reached block's header
    /// is stamped `MARK_STATE`. The marker pushes payload words only when
    /// the block's class can hold a handle. Returns the words that the
    /// marked blocks pushed (`compiler.md` §353 rule 5).
    pub(super) fn arena_mark(
        &mut self,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
        tasks: &CompletedTasks,
    ) -> usize {
        let mut pushed = 0usize;
        while let Some(addr) = work.pop() {
            let (block, payload_size) = if let Some((block, class)) = self.arena_lookup_block(addr)
            {
                // SAFETY: `block` heads a block inside an owned chunk;
                // the state read stays inside it.
                let state = unsafe { (block as *const u64).read() };
                // compiler.md §356 rule 10: a block in the work list of the
                // steps has no mark yet.
                if !matches!(state, LIVE_STATE | GREY_STATE) {
                    // Dead, free-listed, or already marked.
                    continue;
                }
                // Each class that reaches the payload scan has zeroed
                // size-class padding. The final header word carries the
                // allocation attribution.
                (block, (SMALLEST_BLOCK << class) - HEADER_SIZE)
            } else if let Some(a) = self.large.get(&addr) {
                // SAFETY: `base` heads an owned large allocation.
                let state = unsafe { (a.base as *const u64).read() };
                if !matches!(state, LIVE_STATE | GREY_STATE) {
                    // Already marked.
                    continue;
                }
                (a.base, a.payload_size)
            } else {
                continue;
            };
            // SAFETY: `block` heads an owned allocation with a complete
            // header and at least `payload_size` payload bytes.
            let (payload, class_id) = unsafe {
                let payload = block.add(HEADER_SIZE);
                let class_id = header_class_id(block);
                block.cast::<u64>().write(MARK_STATE);
                (payload, class_id)
            };
            if class_id == CLASS_GENERATOR {
                pushed += self.push_task_completion(addr, tasks, work, tracer);
            }
            pushed += self.scan_payload(payload, payload_size, class_id, work, tracer);
        }
        pushed
    }

    /// Ship-tier sweep (§8.1b): walk every chunk's grid up to its bump
    /// watermark — unreached live blocks join their class free list,
    /// marked survivors are restored to `LIVE_STATE` — then free every
    /// unreached large record and restore the marked ones.
    pub(super) fn arena_sweep(&mut self) {
        for ci in 0..self.chunks.len() {
            let (cbase, block_size, class, bump) = {
                let c = &self.chunks[ci];
                (c.base, c.block_size, c.class, c.bump)
            };
            let mut free_head = self.free_heads[class];
            let mut retired_bytes = 0usize;
            for bi in 0..bump {
                // SAFETY: `bi < bump`, so the block is inside the owned
                // chunk; the state word and the payload's first two words
                // (the free-list link and the chunk index) stay inside it.
                unsafe {
                    let block = cbase.add(bi * block_size);
                    let state = (block as *const u64).read();
                    // compiler.md §356 rule 10: the end emptied the work
                    // list, so no block is in it.
                    debug_assert_ne!(state, GREY_STATE, "a block in the work list at the sweep");
                    match state {
                        // A release build keeps a block in the work list.
                        MARK_STATE | GREY_STATE => (block as *mut u64).write(LIVE_STATE),
                        LIVE_STATE => {
                            (block as *mut u64).write(DEAD_STATE);
                            let payload = block.add(HEADER_SIZE);
                            (payload as *mut usize).write(free_head);
                            // compiler.md §346 rule 1a: the chunk index for
                            // the free-list pop.
                            (payload as *mut usize).add(1).write(ci);
                            free_head = payload as usize;
                            retired_bytes += block_size - HEADER_SIZE;
                        }
                        // DEAD_STATE: already on the free list.
                        _ => {}
                    }
                }
            }
            self.free_heads[class] = free_head;
            self.live_bytes_counter = self.live_bytes_counter.saturating_sub(retired_bytes);
        }
        // Cannot `remove`+`dealloc` while iterating, so collect the
        // unreached payload addresses first.
        let mut freed: Vec<usize> = Vec::new();
        for (&addr, a) in &self.large {
            // SAFETY: `base` heads an owned large allocation.
            unsafe {
                let state = (a.base as *const u64).read();
                debug_assert_ne!(state, GREY_STATE, "a block in the work list at the sweep");
                match state {
                    MARK_STATE | GREY_STATE => (a.base as *mut u64).write(LIVE_STATE),
                    _ => freed.push(addr),
                }
            }
        }
        for addr in freed {
            if let Some(a) = self.large.remove(&addr) {
                self.live_bytes_counter = self.live_bytes_counter.saturating_sub(a.payload_size);
                // SAFETY: `base`/`layout` came from `alloc_zeroed` in
                // `arena_alloc_large`; the record was just removed so
                // this frees it exactly once.
                unsafe { dealloc(a.base, a.layout) };
                #[cfg(test)]
                self.stats
                    .large
                    .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    // ----- strings (Q5) -----

    /// Allocates an immutable string and writes its bytes in place.
    ///
    /// The payload is `[len: u64][bytes]`. The writer receives exactly
    /// `len` bytes and must write valid UTF-8 to every byte (compiler.md §327 rule 2).
    /// Returns the payload pointer, or null after an allocation trap.
    pub fn alloc_str_with(
        &mut self,
        len: usize,
        pos_id: u32,
        writer: impl FnOnce(&mut [u8]),
    ) -> *mut u8 {
        let Some(payload_len) = 8usize.checked_add(len) else {
            self.trap(
                TrapKind::AllocationFailure,
                format!("string allocation of {len} bytes is not representable"),
                pos_id,
            );
            return std::ptr::null_mut();
        };
        let p = self.alloc(payload_len, CLASS_STRING, pos_id);
        if p.is_null() {
            return p;
        }
        // SAFETY: `p` points at a live allocation of 8 + len bytes. The
        // byte region starts after the length word and has exactly `len`
        // bytes.
        unsafe {
            (p as *mut u64).write(len as u64);
            let destination = std::slice::from_raw_parts_mut(p.add(8), len);
            writer(destination);
        }
        p
    }

    /// Allocates an immutable string; payload = `[len: u64][bytes]`.
    /// Returns the payload pointer (the string handle).
    ///
    /// `bytes` must contain valid UTF-8 (compiler.md §327 rule 2). This function does not decode.
    /// Rust hosts with other bytes call [`crate::ffi::alloc_str_from_view`].
    pub fn alloc_str(&mut self, bytes: &[u8], pos_id: u32) -> *mut u8 {
        self.alloc_str_with(bytes.len(), pos_id, |destination| {
            destination.copy_from_slice(bytes);
        })
    }

    /// Returns the allocation-free string handle for one BMP scalar.
    ///
    /// The odd tagged value is never dereferenced directly. Its UTF-8
    /// bytes live in [`CODE_POINT_UTF8`] and therefore remain valid for
    /// every Context and across storage in arrays, maps, and fields.
    #[must_use]
    pub(super) fn inline_bmp_code_point(value: char) -> *mut u8 {
        // The only caller branches on this exact BMP bound before calling.
        debug_assert!((value as u32) < BMP_CODE_POINT_COUNT as u32);
        let encoded = value as usize + 1;
        ((encoded << 1) | INLINE_STRING_TAG) as *mut u8
    }

    /// Returns a stable one-code-point string handle for string `for…of`.
    ///
    /// BMP scalars keep the allocation-free tagged form. Each distinct
    /// astral scalar gets one ordinary allocated string per Context; its
    /// intern-map entry is a permanent collection root, so the handle stays
    /// live until the Context is dropped.
    pub(crate) fn code_point(&mut self, value: char, pos_id: u32) -> *mut u8 {
        let scalar = value as u32;
        if scalar < BMP_CODE_POINT_COUNT as u32 {
            return Self::inline_bmp_code_point(value);
        }
        if let Some(&handle) = self.astral_code_points.get(&scalar) {
            return handle as *mut u8;
        }
        let mut storage = [0u8; 4];
        let bytes = value.encode_utf8(&mut storage).as_bytes();
        let handle = self.alloc_str(bytes, pos_id);
        if !handle.is_null() {
            self.astral_code_points.insert(scalar, handle as usize);
        }
        handle
    }

    /// Reads the bytes of a string handle. Allocated strings borrow
    /// immutable Context storage (including interned astral code points);
    /// inline BMP code-point strings borrow immutable process data.
    ///
    /// # Safety
    ///
    /// `handle` must be a live payload produced by
    /// [`Context::alloc_str`] on this context or a tagged BMP handle
    /// produced by string `for…of`.
    #[must_use]
    pub unsafe fn str_bytes(&self, handle: *const u8) -> &[u8] {
        if let Some(scalar) = inline_string_scalar(handle) {
            let len = scalar_utf8_len(scalar);
            // SAFETY: `scalar` is inside the BMP-only static table and
            // each word stores all `len <= 4` UTF-8 bytes contiguously.
            return unsafe {
                std::slice::from_raw_parts(
                    CODE_POINT_UTF8.as_ptr().add(scalar as usize).cast::<u8>(),
                    len,
                )
            };
        }
        // SAFETY: caller guarantees `handle` is a live string payload;
        // its first 8 bytes are the length of the following bytes.
        unsafe {
            let len = (handle as *const u64).read() as usize;
            std::slice::from_raw_parts(handle.add(8), len)
        }
    }

    /// Returns a string view whose borrow is independent of the Context.
    ///
    /// This uses the `subscript_rt_str_concat` invariant: allocating another
    /// Context string does not move an immutable input allocation. The caller
    /// must not delete or collect the handle while the returned view is live.
    ///
    /// # Safety
    ///
    /// `handle` must be a live string handle owned by this Context.
    pub(crate) unsafe fn str_view<'a>(&self, handle: *const u8) -> &'a [u8] {
        // SAFETY: the caller owns the lifetime under the documented invariant.
        let bytes = unsafe { self.str_bytes(handle) };
        // SAFETY: rebuilding from the stable pointer detaches only the Rust
        // borrow; the caller preserves the allocation for the returned view.
        unsafe { std::slice::from_raw_parts(bytes.as_ptr(), bytes.len()) }
    }

    /// The address of a string handle's UTF-8 bytes (the C `const char*`
    /// half of a `(ptr, len)` string view; the length is
    /// [`Context::str_bytes`]`.len()`, also `subscript_rt_str_len`).
    ///
    /// # Safety
    ///
    /// `handle` must be a live string payload of this context.
    #[must_use]
    pub unsafe fn str_data(&self, handle: *const u8) -> *const u8 {
        // SAFETY: forwarded string-handle contract.
        unsafe { self.str_bytes(handle) }.as_ptr()
    }

    /// The address of a dynamic array's element storage (the C `const T*`
    /// half of a `(ptr, count)` descriptor; the count is
    /// [`Context::array_len`]). Null for an array that has never grown.
    ///
    /// # Safety
    ///
    /// `handle` must be a live array payload of this context.
    #[must_use]
    pub unsafe fn array_data(&self, handle: *const u8) -> *const u8 {
        // SAFETY: caller guarantees an array payload.
        unsafe { (*(handle as *const ArrayHeader)).data }
    }

    /// Registers a C-callback binding and returns a stable pointer to it.
    /// The pointer is what a boundary marshaler stores in a C
    /// `void* userdata` slot; the generic trampoline
    /// ([`crate::ffi::subscript_rt_cb_trampoline`]) reads the binding back
    /// through it. Bindings live for the whole Context (the Q13 lifetime
    /// rule), so the pointer stays valid for every later callback.
    ///
    /// Rebinding the same `(code, userdata1, userdata2)` identity returns
    /// the same stable pointer (§14.4a). Boundary callbacks are
    /// non-capturing, so `env` must be null.
    ///
    /// Both userdata slots (§14.4) are stored and delivered to the language
    /// callback; a one-slot callback-info passes `userdata2` as null.
    ///
    /// # Safety
    /// The code remains callable until the Context drops; the environment must be null.
    /// Host userdata must remain valid for each callback that uses it.
    /// Managed userdata can be deleted; the runtime rejects callbacks with deleted managed userdata.
    /// See compiler.md section 263.
    pub unsafe fn bind_callback(
        &mut self,
        code: *const u8,
        env: *const u8,
        userdata1: *mut u8,
        userdata2: *mut u8,
    ) -> *mut u8 {
        debug_assert!(
            env.is_null(),
            "§14.4a callback interning premise requires a null boundary callback env"
        );
        let identity = (code, env, userdata1, userdata2);
        if let Some(&binding) = self.callback_interns.get(&identity) {
            return binding.cast();
        }

        let ctx: *mut Context = self;
        let mut rec = Box::new(CallbackBinding {
            ctx,
            code,
            env,
            userdata1,
            userdata2,
        });
        let ptr: *mut CallbackBinding = &mut *rec;
        self.callbacks.push(rec);
        self.callback_interns.insert(identity, ptr);
        self.advise_binding_count();
        ptr as *mut u8
    }

    /// Interns a string literal by its static data address; repeated
    /// executions of the same literal reuse one allocation. Interned
    /// strings are collection roots.
    ///
    /// # Safety
    ///
    /// `ptr` must point at `len` readable bytes that outlive the
    /// context (the code generator emits them as module data).
    pub unsafe fn intern_literal(&mut self, ptr: *const u8, len: usize, pos_id: u32) -> *mut u8 {
        if let Some(&p) = self.interned.get(&(ptr as usize, len)) {
            return p as *mut u8;
        }
        // SAFETY: caller guarantees `ptr`/`len` is readable.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        let p = self.alloc_str(bytes, pos_id);
        if !p.is_null() {
            self.interned.insert((ptr as usize, len), p as usize);
        }
        p
    }

    // ----- arrays (Q4) -----

    /// Allocates an empty dynamic array with `elem_size`-byte elements whose
    /// elements can hold a handle.
    pub fn array_new(&mut self, elem_size: usize, pos_id: u32) -> *mut u8 {
        self.array_new_of(elem_size, ArrayElements::CanHoldHandle, pos_id)
    }

    /// Allocates an empty dynamic array with `elem_size`-byte elements and
    /// the handle fact `elements` (compiler.md §344 rule 1).
    pub fn array_new_of(
        &mut self,
        elem_size: usize,
        elements: ArrayElements,
        pos_id: u32,
    ) -> *mut u8 {
        let p = self.alloc(
            std::mem::size_of::<ArrayHeader>(),
            elements.header_class(),
            pos_id,
        );
        if p.is_null() {
            return p;
        }
        // SAFETY: `p` is a fresh allocation of ArrayHeader size.
        unsafe {
            (p as *mut ArrayHeader).write(ArrayHeader {
                len: 0,
                cap: 0,
                elem_size: elem_size as u64,
                data: std::ptr::null_mut(),
                holders: 1,
                native_borrows: 0,
            });
        }
        p
    }

    /// Allocates an empty dynamic array with storage for `capacity` elements
    /// whose elements can hold a handle.
    pub fn array_with_capacity(
        &mut self,
        capacity: usize,
        elem_size: usize,
        pos_id: u32,
    ) -> *mut u8 {
        self.array_with_capacity_of(capacity, elem_size, ArrayElements::CanHoldHandle, pos_id)
    }

    /// Allocates an empty dynamic array with storage for `capacity` elements
    /// and the handle fact `elements` (compiler.md §344 rule 1).
    pub fn array_with_capacity_of(
        &mut self,
        capacity: usize,
        elem_size: usize,
        elements: ArrayElements,
        pos_id: u32,
    ) -> *mut u8 {
        let Some(data_size) = capacity.checked_mul(elem_size) else {
            self.trap(
                TrapKind::AllocationFailure,
                "array capacity is not representable",
                pos_id,
            );
            return std::ptr::null_mut();
        };
        let handle = self.array_new_of(elem_size, elements, pos_id);
        if handle.is_null() || capacity == 0 {
            return handle;
        }
        let data = self.alloc(data_size, array_data_class(elements.header_class()), pos_id);
        if data.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: `handle` points to the header that `array_new` created.
        let header = unsafe { &mut *(handle as *mut ArrayHeader) };
        header.cap = capacity as u64;
        header.data = data;
        handle
    }

    /// Allocates zeroed storage for received C elements (compiler.md §325 rule 2).
    /// The array takes the fact of its declared element type (compiler.md
    /// §344 rule 3).
    pub(crate) fn array_received(
        &mut self,
        count: usize,
        stride: usize,
        elements: ArrayElements,
        pos: u32,
    ) -> *mut u8 {
        let handle = self.array_with_capacity_of(count, stride, elements, pos);
        if !handle.is_null() {
            // SAFETY: the new handle owns capacity for all `count` elements.
            unsafe {
                let header = &mut *handle.cast::<ArrayHeader>();
                if count != 0 {
                    std::ptr::write_bytes(header.data, 0, count * stride);
                }
                header.len = count as u64;
            }
        }
        handle
    }

    /// Allocates a byte array and copies `len` bytes from `src`. A byte
    /// array holds no handle (compiler.md §344 rule 5).
    ///
    /// # Safety
    ///
    /// `src` must be readable for `len` bytes.
    pub(crate) unsafe fn array_from_bytes(
        &mut self,
        src: *const u8,
        len: usize,
        pos_id: u32,
    ) -> *mut u8 {
        if len > i32::MAX as usize {
            self.trap(
                TrapKind::Internal,
                "byte-array length exceeds the runtime array limit",
                pos_id,
            );
            return std::ptr::null_mut();
        }
        let handle = self.array_new_of(1, ArrayElements::NoHandle, pos_id);
        if handle.is_null() || len == 0 {
            return handle;
        }
        let data = self.alloc(len, CLASS_ARRAY_DATA_NO_HANDLE, pos_id);
        if data.is_null() {
            self.delete(handle as usize, pos_id);
            return std::ptr::null_mut();
        }
        // SAFETY: the new allocation has `len` writable bytes. The caller
        // guarantees that `src` has `len` readable bytes.
        unsafe { std::ptr::copy_nonoverlapping(src, data, len) };
        // SAFETY: `handle` points to the header that `array_new` created.
        let header = unsafe { &mut *(handle as *mut ArrayHeader) };
        header.len = len as u64;
        header.cap = len as u64;
        header.data = data;
        handle
    }

    /// Returns a pointer to `size` bytes at `offset` in the byte array.
    ///
    /// If the range exceeds the array length, this function traps with
    /// `IndexOutOfBounds` and returns null. The comparison uses 64-bit values.
    ///
    /// # Safety
    ///
    /// `handle` must be a live byte-array payload owned by this context.
    pub(crate) unsafe fn array_byte_range(
        &mut self,
        handle: *mut u8,
        offset: u32,
        size: u32,
        pos_id: u32,
    ) -> *mut u8 {
        if handle.is_null() || !self.require_live_handle(handle as usize, pos_id) {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller guarantees a live array payload.
        let header = unsafe { &*(handle as *const ArrayHeader) };
        let end = u64::from(offset) + u64::from(size);
        if end > header.len {
            self.trap(
                TrapKind::IndexOutOfBounds,
                format!(
                    "byte range at offset {offset} with size {size} exceeds array length {}",
                    header.len
                ),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        if offset == 0 {
            header.data
        } else {
            // SAFETY: the checked nonzero offset is within initialized storage.
            unsafe { header.data.add(offset as usize) }
        }
    }

    /// The length and the handle fact of each live dynamic array, in
    /// address order (compiler.md §344 rule 1). An inspection aid for hosts
    /// and tests.
    #[must_use]
    pub fn live_array_facts(&self) -> Vec<(u64, ArrayElements)> {
        let mut headers: Vec<(usize, u32)> = if self.uses_ship_arena() {
            self.live_blocks()
                // SAFETY: `live_blocks` returns initialized live headers.
                .map(|(base, _)| unsafe { (base.add(HEADER_SIZE) as usize, header_class_id(base)) })
                .chain(self.large.iter().map(|(&payload, allocation)| {
                    // SAFETY: each large record owns a complete header.
                    (payload, unsafe { header_class_id(allocation.base) })
                }))
                .collect()
        } else {
            self.allocations
                .iter()
                .map(|(&payload, allocation)| (payload, allocation.class_id))
                .collect()
        };
        headers.retain(|&(_, class_id)| is_array_class(class_id));
        headers.sort_unstable();
        headers
            .into_iter()
            .map(|(payload, class_id)| {
                // SAFETY: a live array header payload is an ArrayHeader.
                let len = unsafe { (*(payload as *const ArrayHeader)).len };
                (
                    len,
                    ArrayElements::from_no_handle(class_id == CLASS_ARRAY_NO_HANDLE),
                )
            })
            .collect()
    }

    /// The handle fact of a dynamic array (compiler.md §344 rule 1).
    ///
    /// # Safety
    ///
    /// `handle` must be a live array payload owned by this context.
    #[must_use]
    pub unsafe fn array_elements(&self, handle: *const u8) -> ArrayElements {
        // SAFETY: a live payload follows its complete allocation header.
        let class_id = unsafe { header_class_id(handle.sub(HEADER_SIZE)) };
        ArrayElements::from_no_handle(class_id == CLASS_ARRAY_NO_HANDLE)
    }

    /// Array length as `i32`.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context.
    #[must_use]
    pub unsafe fn array_len(&self, handle: *const u8) -> i32 {
        // SAFETY: caller guarantees an array payload.
        unsafe { (*(handle as *const ArrayHeader)).len as i32 }
    }

    /// Element size in bytes of a dynamic array (the size the array was
    /// created with; every element occupies exactly this many bytes).
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context.
    #[must_use]
    pub unsafe fn array_elem_size(&self, handle: *const u8) -> usize {
        // SAFETY: caller guarantees an array payload.
        unsafe { (*(handle as *const ArrayHeader)).elem_size as usize }
    }

    /// Appends one element (copied from `src`); returns the new length,
    /// or -1 after a trap.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context; `src`
    /// must be readable for the array's element size.
    pub unsafe fn array_push(&mut self, handle: *mut u8, src: *const u8, pos_id: u32) -> i32 {
        // SAFETY: forwarded caller contract.
        let (len, grew) = unsafe { self.array_push_unmarked(handle, src, pos_id) };
        if len > 0 {
            // compiler.md §356 rule 14: the new element, or the new storage
            // of a growth, goes to the work list during a mark.
            // SAFETY: the push proved a live array payload.
            unsafe {
                let data = if grew {
                    std::ptr::null()
                } else {
                    self.array_data(handle)
                };
                self.mark_written_elements(handle, data, len as usize - 1, len as usize);
            }
        }
        len
    }

    /// [`Context::array_push`] with no work of `compiler.md` §356 rule 14.
    /// Returns the new length, or -1 after a trap, and true if the storage
    /// grew.
    ///
    /// # Safety
    ///
    /// The contract of [`Context::array_push`].
    pub(super) unsafe fn array_push_unmarked(
        &mut self,
        handle: *mut u8,
        src: *const u8,
        pos_id: u32,
    ) -> (i32, bool) {
        if handle.is_null() || !self.require_live_handle(handle as usize, pos_id) {
            return (-1, false);
        }
        let mut grew = false;
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        if h.len == h.cap {
            if h.native_borrows != 0 {
                self.trap(
                    TrapKind::ArrayStorageBorrowed,
                    TrapKind::ArrayStorageBorrowed.message(None),
                    pos_id,
                );
                return (-1, false);
            }
            grew = true;
            let new_cap = if h.cap == 0 { 4 } else { h.cap * 2 };
            let elem = h.elem_size as usize;
            // compiler.md §344 rule 4: growth keeps the fact of the array.
            // SAFETY: the caller guarantees a live array payload, so its
            // allocation header precedes it.
            let data_class = array_data_class(unsafe { header_class_id(handle.sub(HEADER_SIZE)) });
            let new_data = self.alloc(new_cap as usize * elem, data_class, pos_id);
            if new_data.is_null() {
                return (-1, false);
            }
            // Re-borrow after alloc (`self` was mutably borrowed).
            // SAFETY: as above.
            let h = unsafe { &mut *(handle as *mut ArrayHeader) };
            if !h.data.is_null() {
                // SAFETY: old data holds `len * elem` initialized bytes;
                // new data is at least twice as large.
                unsafe {
                    std::ptr::copy_nonoverlapping(h.data, new_data, h.len as usize * elem);
                }
                let old = h.data as usize;
                // Retire the old storage (internal, so not a trap path).
                if self.uses_ship_arena() {
                    // Ship tier (§8.1b): retired data blocks flow through
                    // the same free-list/large-record release path as
                    // `delete`, so array growth does not accumulate.
                    self.arena_release(old);
                } else {
                    let disposed = self.dispose_dev_allocation(old);
                    match disposed {
                        Some(class_id) if is_array_data_class(class_id) => {}
                        Some(_) => {
                            self.trap(
                                TrapKind::Internal,
                                "array growth found a non-array storage allocation",
                                pos_id,
                            );
                            return (-1, false);
                        }
                        None => {
                            self.trap(
                                TrapKind::Internal,
                                "array storage disappeared while growing it",
                                pos_id,
                            );
                            return (-1, false);
                        }
                    }
                }
            }
            // SAFETY: as above.
            let h = unsafe { &mut *(handle as *mut ArrayHeader) };
            h.data = new_data;
            h.cap = new_cap;
        }
        // SAFETY: as above.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        let elem = h.elem_size as usize;
        // SAFETY: `data` has capacity for `cap` elements and len < cap;
        // `src` is readable for `elem` bytes per the caller contract.
        unsafe {
            std::ptr::copy_nonoverlapping(src, h.data.add(h.len as usize * elem), elem);
        }
        h.len += 1;
        (h.len as i32, grew)
    }

    /// Removes the last element, copying it to `dst`. Traps on empty.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context; `dst`
    /// must be writable for the array's element size and must not overlap
    /// the array's data storage. The vacated slot is zeroed after the copy.
    pub unsafe fn array_pop(&mut self, handle: *mut u8, dst: *mut u8, pos_id: u32) {
        // SAFETY: the caller contract of this function.
        if unsafe { self.array_pop_or_absent(handle, dst, pos_id) } == Some(false) {
            self.trap(TrapKind::EmptyPop, "pop() on an empty array", pos_id);
        }
    }

    /// Removes the last element, copying it to `dst`. An empty array
    /// leaves `dst` unchanged and returns `Some(false)` (compiler.md
    /// §196.1). A null or dead handle returns `None`, after the trap of
    /// the dead handle.
    ///
    /// # Safety
    ///
    /// The contract of [`Context::array_pop`].
    pub unsafe fn array_pop_or_absent(
        &mut self,
        handle: *mut u8,
        dst: *mut u8,
        pos_id: u32,
    ) -> Option<bool> {
        if handle.is_null() || !self.require_live_handle(handle as usize, pos_id) {
            return None;
        }
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        if h.len == 0 {
            return Some(false);
        }
        h.len -= 1;
        let elem = h.elem_size as usize;
        // SAFETY: the removed slot holds an initialized element; `dst`
        // is writable per the caller contract. The slot remains inside the
        // data allocation after the copy.
        unsafe {
            let removed = h.data.add(h.len as usize * elem);
            std::ptr::copy_nonoverlapping(removed, dst, elem);
            std::ptr::write_bytes(removed, 0, elem);
        }
        Some(true)
    }

    /// Returns the address of element `idx`, or null after an
    /// out-of-bounds trap.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context.
    pub unsafe fn array_elem_ptr(&mut self, handle: *mut u8, idx: i32, pos_id: u32) -> *mut u8 {
        if handle.is_null() || !self.require_live_handle(handle as usize, pos_id) {
            return std::ptr::null_mut();
        }
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &*(handle as *const ArrayHeader) };
        if idx < 0 || idx as u64 >= h.len {
            let len = h.len;
            self.trap(
                TrapKind::IndexOutOfBounds,
                TrapKind::IndexOutOfBounds.message(Some((idx, len))),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: 0 <= idx < len <= cap.
        unsafe { h.data.add(idx as usize * h.elem_size as usize) }
    }

    /// Appends `count` consecutive elements from `src`.
    ///
    /// # Safety
    ///
    /// `handle` must be a live array. `src` must contain `count` readable
    /// elements with the receiver's element size.
    pub(crate) unsafe fn array_extend(
        &mut self,
        handle: *mut u8,
        src: *const u8,
        count: usize,
        pos_id: u32,
    ) -> bool {
        if count == 0 && src.is_null() {
            return true;
        }
        if handle.is_null() || !self.require_live_handle(handle as usize, pos_id) {
            return false;
        }
        if count == 0 {
            return true;
        }
        if src.is_null() {
            return false;
        }
        // SAFETY: the receiver follows this function's contract.
        let width = unsafe { self.array_elem_size(handle) };
        // SAFETY: the receiver is live.
        let first = unsafe { self.array_len(handle) }.max(0) as usize;
        let mut grew = false;
        let mut complete = true;
        for index in 0..count {
            // SAFETY: the caller supplies `count` consecutive elements.
            let element = unsafe { src.add(index * width) };
            // SAFETY: forwarded receiver and element contracts.
            let (len, grown) = unsafe { self.array_push_unmarked(handle, element, pos_id) };
            grew |= grown;
            if len < 0 {
                complete = false;
                break;
            }
        }
        // compiler.md §356 rule 14: one test of the flag for the extension.
        // SAFETY: the receiver is live.
        unsafe {
            let data = if grew {
                std::ptr::null()
            } else {
                self.array_data(handle)
            };
            let len = self.array_len(handle).max(0) as usize;
            self.mark_written_elements(handle, data, first, len);
        }
        complete
    }

    /// Shrinks an array to `new_len` without copying removed elements.
    ///
    /// # Safety
    ///
    /// `handle` must be a live array and `new_len` must not exceed its length.
    pub(crate) unsafe fn array_truncate(&mut self, handle: *mut u8, new_len: usize, pos_id: u32) {
        if handle.is_null() || !self.require_live_handle(handle as usize, pos_id) {
            return;
        }
        // SAFETY: liveness and the caller contract establish the header.
        let header = unsafe { &mut *handle.cast::<ArrayHeader>() };
        debug_assert!(new_len <= header.len as usize);
        let old_len = header.len as usize;
        let new_len = new_len.min(old_len);
        let elem_size = header.elem_size as usize;
        let cleared_range = new_len
            .checked_mul(elem_size)
            .zip((old_len - new_len).checked_mul(elem_size));
        // SAFETY: `data` contains `old_len * elem_size` initialized bytes.
        // The range starts at the new length and ends at the old length.
        if let Some((start, bytes)) = cleared_range.filter(|(_, bytes)| *bytes != 0) {
            unsafe { std::ptr::write_bytes(header.data.add(start), 0, bytes) };
        }
        header.len = new_len as u64;
    }
}

/// A collection release plan whose allocation storage remains live until sweep.
#[must_use]
#[non_exhaustive]
pub(super) struct CollectionReleases {
    /// Handle leaves. The consumer resolves and sorts these by its task ids.
    pub handles: Vec<usize>,
    storage: Vec<usize>,
    // compiler.md §348 rule 4: the completed tasks that the mark did not
    // reach. The release of `handles` ends each of them.
    ending_tasks: Vec<usize>,
    trace: Vec<String>,
}

impl Context {
    /// Marks the heap, calls the owner's release consumer, and always sweeps before return.
    /// The accessor selects the owner's Context. The consumer releases every handle leaf.
    /// An early return or unwind also completes the sweep.
    pub fn with_collection<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        consume: impl FnOnce(&mut T, &mut [usize]) -> R,
    ) -> R {
        Self::collection_with_late_marks(owner, context, &[], consume)
    }

    /// [`Context::with_collection`], where the mark reads the words of
    /// `late` after the release plan is made (`compiler.md` §348 rule 4).
    pub(super) fn collection_with_late_marks<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        late: &[usize],
        consume: impl FnOnce(&mut T, &mut [usize]) -> R,
    ) -> R {
        struct Guard<'a, T> {
            owner: &'a mut T,
            context: fn(&mut T) -> &mut Context,
            plan: Option<CollectionReleases>,
        }
        impl<T> Drop for Guard<'_, T> {
            fn drop(&mut self) {
                if let Some(plan) = self.plan.take() {
                    (self.context)(self.owner).finish_collection(plan);
                }
            }
        }
        let mut plan = context(owner).prepare_collection(mark_trace_target(), late);
        let mut handles = std::mem::take(&mut plan.handles);
        let guard = Guard {
            owner,
            context,
            plan: Some(plan),
        };
        consume(guard.owner, &mut handles)
    }

    /// Frees the release plan's array storage and sweeps the marked heap.
    /// The caller must release every handle from the plan before this call.
    fn finish_collection(&mut self, plan: CollectionReleases) {
        self.keep_unended_tasks(&plan.ending_tasks);
        for address in plan.storage {
            if self.is_live(address) {
                self.delete(address, 0);
            }
        }
        if self.uses_ship_arena() {
            self.arena_sweep();
            // compiler.md §356 rule 5: the sweep ends the cycle.
            self.clear_mark_cycle();
        } else {
            let retiring = if self.freed_handle_diagnostics
                && self.freed_handle_diagnostics_max_retained_bytes == usize::MAX
            {
                self.allocations
                    .values()
                    .filter(|allocation| !allocation.marked)
                    .count()
            } else {
                0
            };
            self.sweep_dev_allocations(retiring);
        }
        self.sweep_regex_values();
    }

    pub(super) fn collect_with_trace(
        &mut self,
        trace_target: Option<MarkTraceTarget>,
    ) -> Vec<String> {
        if self.allocations.is_empty() && self.chunks.is_empty() && self.large.is_empty() {
            self.sweep_regex_values();
            // compiler.md §356 rule 5: with no block, the end has no sweep.
            self.clear_mark_cycle();
            return Vec::new();
        }
        let mut plan = self.prepare_collection(trace_target, &[]);
        plan.handles.sort_unstable_by_key(|handle| {
            self.async_frames.get(handle).map_or(0, |meta| meta.task_id)
        });
        for &handle in &plan.handles {
            // SAFETY: descriptions contain native registered frame keys.
            unsafe { self.async_release(handle as *mut u8, 0) };
        }
        let trace = std::mem::take(&mut plan.trace);
        self.finish_collection(plan);
        trace
    }
}

impl Context {
    pub(crate) fn collect_at(&mut self, pos: u32) {
        let trapped = self.trapped();
        self.collect();
        if !trapped {
            if let Some(trap) = self
                .trap
                .as_mut()
                .filter(|trap| trap.kind == TrapKind::UncaughtException)
            {
                trap.pos_id = pos;
            }
        }
    }
}

impl Context {
    /// Returns root words and the words reachable through live allocation payloads.
    /// This walk preserves the collector's scalar-payload exclusions.
    pub fn reachable_words(&self, roots: &[usize]) -> Vec<usize> {
        let mut work = roots.to_vec();
        work.extend(
            self.boundary_borrows
                .iter()
                .filter_map(|record| match record {
                    BoundaryBorrow::Value { value, .. } => Some(*value),
                    BoundaryBorrow::Scope { .. } => None,
                }),
        );
        for &(base, count) in self.roots.iter().chain(&self.shadow) {
            for index in 0..count {
                // SAFETY: registered root ranges remain live until their owner removes them.
                work.push(unsafe { ((base + index * 8) as *const usize).read_unaligned() });
            }
        }
        let mut seen = std::collections::HashSet::new();
        while let Some(address) = work.pop() {
            if !seen.insert(address) {
                continue;
            }
            let Some(size) = self.live_payload_size(address) else {
                continue;
            };
            let class_id = if !self.uses_ship_arena() {
                self.allocations
                    .get(&address)
                    .map(|allocation| allocation.class_id)
            } else if let Some((block, _)) = self.arena_lookup_block(address) {
                // SAFETY: live payload membership establishes the allocation header.
                Some(unsafe { header_class_id(block) })
            } else {
                self.large
                    .get(&address)
                    .map(|allocation| unsafe { header_class_id(allocation.base) })
            };
            if let Some(class_id) = class_id {
                self.scan_payload(address as *const u8, size, class_id, &mut work, &mut None);
            }
        }
        seen.into_iter().collect()
    }
}
