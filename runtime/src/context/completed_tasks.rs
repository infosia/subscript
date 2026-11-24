//! A completed task is not a collection root by itself (`compiler.md` §348).
//!
//! A completed task is a registered invocation frame with a completion, no
//! waiter, and no scheduler state. The mark reads its completion only when
//! the mark reaches it (rule 2). After the mark, a completed task that the
//! mark did not reach becomes a root if an outside holder owns a count of
//! it (rule 3). The other completed tasks lose their counts through the
//! release of unreachable counted fields and end (rule 4).

use std::collections::{HashMap, HashSet};

use super::async_scheduler::AsyncKind;
use super::counted::object_fields;
use super::*;

/// The completed tasks of one collection (`compiler.md` §348 terms).
///
/// A frame with a count of zero is not in the set: no release can end it,
/// so it stays a root.
#[derive(Default)]
pub(super) struct CompletedTasks {
    frames: HashSet<usize, BuildHasherDefault<AddressHasher>>,
}

impl CompletedTasks {
    /// True when `frame` is a completed task of this collection.
    pub(super) fn contains(&self, frame: usize) -> bool {
        !self.frames.is_empty() && self.frames.contains(&frame)
    }
}

impl Context {
    /// The completed tasks of the registered frames (`compiler.md` §348
    /// terms). Each other frame stays a root (rule 5).
    pub(super) fn completed_tasks(&self) -> CompletedTasks {
        let mut frames: HashSet<usize, BuildHasherDefault<AddressHasher>> = self
            .async_frames
            .iter()
            .filter(|(&frame, meta)| {
                matches!(meta.kind, AsyncKind::Invocation)
                    && meta.completion.is_some()
                    && meta.waiters.is_empty()
                    // SAFETY: a registered frame has its count word at offset four.
                    && unsafe { self.async_count(frame as *const u8) } != 0
            })
            .map(|(&frame, _)| frame)
            .collect();
        if frames.is_empty() {
            return CompletedTasks { frames };
        }
        // §348 terms: a frame in a scheduler state is not a completed task.
        for job in &self.async_ready {
            frames.remove(&(job.handle() as usize));
        }
        if let Some((job, _)) = &self.async_trapping {
            frames.remove(&(job.handle() as usize));
        }
        for frame in self.async_parked.iter().chain(&self.async_stopped) {
            frames.remove(&(*frame as usize));
        }
        for frame in &self.active_async_frames {
            frames.remove(frame);
        }
        for meta in self.async_frames.values() {
            for job in &meta.waiters {
                frames.remove(&(job.handle() as usize));
            }
        }
        CompletedTasks { frames }
    }

    /// `compiler.md` §348 rule 2: the mark reached `frame`. If it is a
    /// completed task, the mark reads its completion value and its
    /// completion Error object. Both are Context-owned storage outside the
    /// frame payload, so the payload scan does not read them. Returns the
    /// count of the words that it pushed (`compiler.md` §353 rule 5).
    pub(super) fn push_task_completion(
        &self,
        frame: usize,
        tasks: &CompletedTasks,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
    ) -> usize {
        if !tasks.contains(frame) {
            return 0;
        }
        let Some(completion) = self
            .async_frames
            .get(&frame)
            .and_then(|meta| meta.completion.as_ref())
        else {
            return 0;
        };
        let before = work.len();
        if let Some(bytes) = completion.value() {
            for (word, chunk) in bytes
                .chunks_exact(core::mem::size_of::<usize>())
                .enumerate()
            {
                // SAFETY: `chunk` is exactly one native word of Context-owned
                // completion storage; an unaligned read keeps aggregate
                // layouts and shows each managed handle to the marker.
                let address = unsafe { chunk.as_ptr().cast::<usize>().read_unaligned() };
                self.push_mark_word(
                    work,
                    tracer,
                    address,
                    MarkSource::Root {
                        set: "completion",
                        index: 0,
                        word,
                    },
                );
            }
        }
        if let Some(object) = completion.exception_object() {
            self.push_mark_word(
                work,
                tracer,
                object,
                MarkSource::Root {
                    set: "completion_exception",
                    index: 0,
                    word: 0,
                },
            );
        }
        work.len() - before
    }

    /// `compiler.md` §348 rule 3, in one pass over the completed tasks that
    /// the mark did not reach, in the order of their task ids. A task whose
    /// count is above its heap holders in unreachable objects becomes a
    /// root, and the mark continues from it before the pass reads the next
    /// task.
    ///
    /// The holders are counted once, before the pass. A mark in the pass
    /// only makes unreachable objects reachable. If the mark reaches a
    /// holder of a task, it reads the handle in that holder and reaches the
    /// task. So a task that is not marked when the pass reads it has the
    /// holders of the count before the pass, and the pass skips each task
    /// that is marked.
    pub(super) fn root_tasks_with_outside_holders(
        &mut self,
        tasks: &CompletedTasks,
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
    ) {
        let mut unmarked = self.unmarked_tasks(tasks);
        if unmarked.is_empty() {
            return;
        }
        unmarked.sort_unstable_by_key(|frame| {
            self.async_frames.get(frame).map_or(0, |meta| meta.task_id)
        });
        let holders = self.heap_holders(&|object| self.collection_marked(object), &|leaf| {
            tasks.contains(leaf)
        });
        for (index, frame) in unmarked.into_iter().enumerate() {
            if self.collection_marked(frame) {
                continue;
            }
            // SAFETY: a completed task is a registered frame.
            let count = unsafe { self.async_count(frame as *const u8) };
            if holders.get(&frame).copied().unwrap_or(0) < count {
                self.push_mark_word(
                    work,
                    tracer,
                    frame,
                    MarkSource::Root {
                        set: "completed_task_outside_holder",
                        index,
                        word: 0,
                    },
                );
                self.drain_mark(work, tracer, tasks);
            }
        }
    }

    /// The completed tasks that the mark did not reach.
    pub(super) fn unmarked_tasks(&self, tasks: &CompletedTasks) -> Vec<usize> {
        tasks
            .frames
            .iter()
            .copied()
            .filter(|frame| !self.collection_marked(*frame))
            .collect()
    }

    /// `compiler.md` §348 rule 3 for a tier with a handle registry of its
    /// own: for each handle in `tasks`, the counts that the release of
    /// unreachable counted fields, array elements, and Map values would
    /// release. An object or Map is unreachable if `reached` does not
    /// contain its address. The walk changes nothing.
    #[must_use]
    pub fn completed_task_holders(
        &self,
        reached: &HashSet<usize>,
        tasks: &HashSet<usize>,
    ) -> HashMap<usize, u32> {
        if tasks.is_empty() {
            return HashMap::new();
        }
        self.heap_holders(&|object| reached.contains(&object), &|leaf| {
            tasks.contains(&leaf)
        })
    }

    /// `compiler.md` §348 rule 3: for each leaf that `is_task` selects, the
    /// counts that the release of unreachable counted fields and Map values
    /// would release. The walk is the walk of
    /// [`Context::take_object_releases`] and of the Map release, and it
    /// changes nothing: an array holder count is decreased in a local table
    /// only. Rule 8: it reads only the unreachable objects that have a
    /// description, and the unreachable counted Maps.
    ///
    /// A count that is too small keeps a task; it never ends one.
    fn heap_holders(
        &self,
        reached: &dyn Fn(usize) -> bool,
        is_task: &dyn Fn(usize) -> bool,
    ) -> HashMap<usize, u32> {
        let mut holders = HashMap::new();
        let mut arrays = HashMap::new();
        for (&object, description) in &self.object_descriptions {
            if reached(object) || !self.is_live(object) {
                continue;
            }
            #[cfg(test)]
            self.stats
                .completed_task_objects_read
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let generator;
            let description: &[u8] = if description.as_slice() == 0u64.to_ne_bytes()
                // SAFETY: a live payload has a complete header before it.
                && unsafe { header_class_id((object as *const u8).sub(HEADER_SIZE)) }
                    == CLASS_GENERATOR
            {
                // SAFETY: the generator marker refers to a live generated frame.
                generator = unsafe { self.generator_description(object as *mut u8) };
                &generator
            } else {
                description
            };
            // The release traps on a malformed description and releases
            // nothing for that object, so the count reads nothing either.
            let Ok(fields) = object_fields(description) else {
                continue;
            };
            for (offset, cursor) in fields {
                // SAFETY: describe_object requires live fields, and
                // `object_fields` checked every node of the description.
                unsafe {
                    self.count_release_leaves(
                        (object as *const u8).add(offset),
                        description.as_ptr().add(cursor),
                        is_task,
                        &mut arrays,
                        &mut holders,
                    );
                }
            }
        }
        for (&map, description) in &self.counted_maps {
            if reached(map) || !self.is_live(map) {
                continue;
            }
            #[cfg(test)]
            self.stats
                .completed_task_objects_read
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // The Map release uses the stored description of a tier with a
            // registry of its own, and the header description otherwise.
            let description = if description.is_empty() {
                std::ptr::null()
            } else {
                description.as_ptr()
            };
            // SAFETY: a live counted Map has a live header and entry
            // storage, and each value matches the value description.
            unsafe {
                crate::assocops::counted_values(map as *const u8, description, |value, node| {
                    self.count_release_leaves(value, node, is_task, &mut arrays, &mut holders)
                });
            }
        }
        holders
    }

    /// The walk of [`Context::counted_release_leaves`] with no change. A
    /// handle leaf that is a completed task adds one to its holders. An
    /// array adds one to its releases in `arrays`; its elements are read
    /// when the releases reach its holder count, as the release reads them
    /// when the count reaches zero.
    ///
    /// # Safety
    /// `value` matches the static recursive `description`, and all container
    /// storage is live.
    unsafe fn count_release_leaves(
        &self,
        value: *const u8,
        description: *const u8,
        is_task: &dyn Fn(usize) -> bool,
        arrays: &mut HashMap<usize, u64>,
        holders: &mut HashMap<usize, u32>,
    ) {
        // SAFETY: generated descriptions consist of three u64 words per node.
        let word =
            |offset| unsafe { description.add(offset).cast::<u64>().read_unaligned() as usize };
        // SAFETY: a container node has its child node after its own three words.
        let child = unsafe { description.add(24) };
        match word(0) {
            1 | 5 => {
                // SAFETY: the description states a handle at `value`.
                let leaf = unsafe { value.cast::<usize>().read_unaligned() };
                if is_task(leaf) {
                    // §348 rule 3: a count that is too small only keeps a task.
                    let count = holders.entry(leaf).or_insert(0);
                    *count = count.saturating_add(1);
                }
            }
            2 => {
                // SAFETY: the description states an array handle at `value`.
                let array = unsafe { value.cast::<*mut u8>().read_unaligned() };
                if array.is_null() {
                    return;
                }
                // SAFETY: the caller states that container storage is live.
                let header = unsafe { &*array.cast::<ArrayHeader>() };
                let releases = arrays.entry(array as usize).or_insert(0);
                *releases = releases.saturating_add(1);
                // The release decreases the holder count by one, not under
                // zero, and reads the elements when the count is zero.
                if *releases < u64::from(header.holders) {
                    return;
                }
                let (len, stride, data) =
                    (header.len as usize, header.elem_size as usize, header.data);
                for index in 0..len {
                    // SAFETY: each element below `len` lies in the live storage.
                    unsafe {
                        self.count_release_leaves(
                            data.add(index * stride),
                            child,
                            is_task,
                            arrays,
                            holders,
                        )
                    };
                }
            }
            3 => {
                for index in 0..word(16) {
                    // SAFETY: the node states `word(16)` elements of `word(8)` bytes.
                    unsafe {
                        self.count_release_leaves(
                            value.add(index * word(8)),
                            child,
                            is_task,
                            arrays,
                            holders,
                        )
                    };
                }
            }
            // SAFETY: the node states a tag byte at `value`.
            4 if unsafe { value.read() } == 0 => {
                // SAFETY: the node states the value at offset `word(8)`.
                unsafe {
                    self.count_release_leaves(value.add(word(8)), child, is_task, arrays, holders)
                };
            }
            _ => {}
        }
    }

    /// `compiler.md` §348 rule 4 and §342 rule 12: a completed task that the
    /// mark did not reach ends in this collection. If its completion is an
    /// exception that no `await` raised, the release records the failure,
    /// and the record keeps the Error object. So the mark reads the Error
    /// object after the release plan is made: a holder of the task that only
    /// the Error object reaches is released first. The mark also reads the
    /// words of `late`, the Error objects of the ending tasks of a tier with
    /// a handle registry of its own. Returns the tasks that end.
    pub(super) fn mark_ending_task_failures(
        &mut self,
        tasks: &CompletedTasks,
        late: &[usize],
        work: &mut Vec<usize>,
        tracer: &mut Option<MarkTracer>,
    ) -> Vec<usize> {
        let ending = self.unmarked_tasks(tasks);
        for (index, frame) in ending.iter().enumerate() {
            let object =
                self.async_frames
                    .get(frame)
                    .and_then(|meta| match meta.completion.as_ref()? {
                        crate::exception::Completion::Exception(payload) if !payload.observed => {
                            Some(payload.exception.object)
                        }
                        _ => None,
                    });
            if let Some(object) = object {
                self.push_mark_word(
                    work,
                    tracer,
                    object,
                    MarkSource::Root {
                        set: "ending_task_failure",
                        index,
                        word: 0,
                    },
                );
            }
        }
        for (index, &object) in late.iter().enumerate() {
            self.push_mark_word(
                work,
                tracer,
                object,
                MarkSource::Root {
                    set: "ending_task_failure_late",
                    index,
                    word: 0,
                },
            );
        }
        if !work.is_empty() {
            self.drain_mark(work, tracer, tasks);
        }
        ending
    }

    /// [`Context::with_collection`] for a tier with a handle registry of its
    /// own (`compiler.md` §348 rule 4). `failures` are the Error objects of
    /// the completed tasks that end in this collection with a failure that
    /// no `await` raised. The mark reads them after the release plan is
    /// made, so a holder of a task that only its Error object reaches is in
    /// the plan, and the sweep keeps the Error object for the record of the
    /// failure (§342 rule 12).
    pub fn with_collection_after_task_failures<T, R>(
        owner: &mut T,
        context: fn(&mut T) -> &mut Context,
        failures: &[usize],
        consume: impl FnOnce(&mut T, &mut [usize]) -> R,
    ) -> R {
        Self::collection_with_late_marks(owner, context, failures, consume)
    }

    /// `compiler.md` §348 rule 4: after the release, each task that the
    /// mark did not reach has ended. A task that is still registered here
    /// lost fewer counts than its holders in unreachable objects, which is
    /// an internal defect. The sweep must not free a registered frame or
    /// an object that its completion reaches (rule 2), so the frame and its
    /// completion stay, and the Context traps.
    pub(super) fn keep_unended_tasks(&mut self, ending: &[usize]) {
        let unended: HashSet<usize, BuildHasherDefault<AddressHasher>> = ending
            .iter()
            .copied()
            .filter(|frame| {
                self.async_frames.contains_key(frame) && !self.collection_marked(*frame)
            })
            .collect();
        if unended.is_empty() {
            return;
        }
        let unended = CompletedTasks { frames: unended };
        let mut work = Vec::new();
        let mut tracer = None;
        for &frame in &unended.frames {
            self.mark_block(frame);
            self.push_task_completion(frame, &unended, &mut work, &mut tracer);
        }
        self.drain_mark(&mut work, &mut tracer, &unended);
        if !self.trapped() {
            self.trap(
                TrapKind::Internal,
                "a completed task outlived the release of its unreachable holders",
                0,
            );
        }
    }

    /// Marks the block of one live allocation, and not its payload.
    fn mark_block(&mut self, payload: usize) {
        if !self.uses_ship_arena() {
            if let Some(allocation) = self.allocations.get_mut(&payload) {
                allocation.marked = true;
            }
            return;
        }
        let block = self
            .arena_lookup_block(payload)
            .map(|(block, _)| block)
            .or_else(|| self.large.get(&payload).map(|allocation| allocation.base));
        if let Some(block) = block {
            // SAFETY: an exact allocation lookup proves that the header is
            // readable and writable.
            // compiler.md §357 rule 2: the words of the Context.
            unsafe {
                let state = block.cast::<u64>().read();
                if state == self.mark_cycle.live_word || state == GREY_STATE {
                    block.cast::<u64>().write(self.mark_cycle.marked_word);
                }
            }
        }
    }
}
