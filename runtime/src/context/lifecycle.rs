use super::*;

impl Context {
    /// Creates an empty context.
    ///
    /// The dev-JIT builds its Context this way. Individual allocations
    /// preserve exact requested-byte accounting (§18.2d), while
    /// `Context.free` and `Context.collect` release them immediately by
    /// default. Enable freed-handle diagnostics before the first allocation
    /// to retain and poison freed allocations at or above its configured
    /// payload threshold within its retention budget instead.
    #[must_use]
    pub fn new() -> Box<Context> {
        Self::with_tier(false)
    }

    /// Creates an empty ship-tier context (§8.1a/§8.1b).
    ///
    /// `Context.free` and `Context.collect` release immediately by
    /// default: a size-classed block goes back to its arena free list and
    /// a large allocation is freed outright. Use-after-delete and double
    /// delete are undefined (Q6/§8.1b), not trapped. The AOT host entry
    /// ([`crate::ffi::subscript_rt_ctx_new`]) builds its Context this way.
    #[must_use]
    pub fn new_releasing() -> Box<Context> {
        Self::with_tier(true)
    }

    pub(super) fn with_tier(ship_arena: bool) -> Box<Context> {
        let context_id = super::host_operation::next_context_id();
        let mut context = Box::new(Context {
            trap_flag: 0,
            reload_epoch: 0,
            fn_table: std::ptr::null(),
            globals: std::ptr::null_mut(),
            module_globals: None,
            workers: WorkerSet::default(),
            script_depth: 0,
            async_ready: VecDeque::new(),
            async_parked: VecDeque::new(),
            async_trapping: None,
            async_stopped: Vec::new(),
            active_async_frames: Vec::new(),
            async_frames: HashMap::default(),
            next_async_task_id: 1,
            context_id,
            next_host_operation_id: 1,
            host_operations: HashMap::new(),
            task_groups: HashMap::new(),
            live_bytes_counter: 0,
            allocations: HashMap::new(),
            boundary_scratch: Vec::new(),
            dead_allocations: AddressSet::default(),
            retained_allocations: VecDeque::new(),
            retained_bytes: 0,
            stdout: Vec::new(),
            print_observer: None,
            print_observer_userdata: std::ptr::null_mut(),
            trap: None,
            pending_exception: None,
            parked_exceptions: Vec::new(),
            trap_observer: None,
            trap_observer_userdata: std::ptr::null_mut(),
            trap_observer_active: false,
            diagnostics_observer: None,
            diagnostics_observer_userdata: std::ptr::null_mut(),
            binding_count_advisory_threshold: u64::MAX,
            interned: HashMap::new(),
            astral_code_points: HashMap::new(),
            shadow: Vec::new(),
            roots: Vec::new(),
            object_descriptions: HashMap::new(),
            counted_maps: HashMap::new(),
            callbacks: Vec::new(),
            callback_interns: HashMap::new(),
            registrations: crate::registration::RegistrationSet::default(),
            json_builders: crate::json::JsonBuilders::default(),
            json_parsers: crate::json::JsonParsers::default(),
            ship_arena,
            freed_handle_diagnostics: false,
            freed_handle_diagnostics_min_payload_bytes: 0,
            freed_handle_diagnostics_max_retained_bytes: 0,
            allocation_started: false,
            rng: crate::math::Rng::new(crate::math::DEFAULT_RANDOM_SEED),
            now_override: None,
            regex_budget: 100_000,
            regex: crate::regexops::RegexStore::default(),
            alloc_fail_countdown: None,
            chunks: Vec::new(),
            chunk_map: Vec::new(),
            free_heads: [0; NUM_CLASSES],
            open: [None; NUM_CLASSES],
            large: HashMap::new(),
            #[cfg(test)]
            stats: Default::default(),
        });
        if context_id == 0 {
            context.trap(TrapKind::Internal, "Context id exhausted", 0);
        }
        context
    }

    /// Creates the dedicated Context owned by one runtime worker.
    ///
    /// Called only inside the new worker thread so the Context is created,
    /// driven, and released without crossing a thread boundary.
    pub(crate) fn new_worker(releasing: bool) -> Box<Context> {
        Self::with_tier(releasing)
    }

    pub(crate) fn worker_spawn(
        &mut self,
        init: WorkerInit,
        entry: WorkerEntry,
        input_descriptor: QueueDescriptor,
        output_descriptor: QueueDescriptor,
    ) -> *mut Worker {
        if self.trapped() {
            return std::ptr::null_mut();
        }
        // SAFETY: the raw reload indirection-table pointer cannot implement
        // Send, so it crosses the thread boundary losslessly as `usize`.
        // ReloadSession refuses swaps while a worker is live and drops this
        // Context (which joins every worker) before freeing retained JIT
        // modules, keeping the table and every address it contains valid.
        let fn_table = self.fn_table as usize;
        match self.workers.spawn(
            init,
            entry,
            input_descriptor,
            output_descriptor,
            self.ship_arena,
            fn_table,
        ) {
            Ok(worker) => worker,
            Err(error) => {
                self.trap(
                    TrapKind::AllocationFailure,
                    format!("worker thread creation failed: {error}"),
                    0,
                );
                std::ptr::null_mut()
            }
        }
    }

    pub(crate) unsafe fn worker_post(&mut self, worker: *mut Worker, payload: *const u8) -> bool {
        if self.trapped() {
            return false;
        }
        // SAFETY: the FFI caller supplies one readable fixed-size payload.
        match unsafe { self.workers.post(self, worker, payload) } {
            Some(PostResult::Posted) => true,
            Some(PostResult::Closed) => false,
            Some(PostResult::NullPayload) => {
                self.trap(
                    TrapKind::Internal,
                    "worker post received a null non-empty payload",
                    0,
                );
                false
            }
            Some(PostResult::AllocationFailed) => {
                self.trap(
                    TrapKind::AllocationFailure,
                    "worker message record allocation failed",
                    0,
                );
                false
            }
            None => {
                self.trap(
                    TrapKind::Internal,
                    "worker handle is not owned by Context",
                    0,
                );
                false
            }
        }
    }

    pub(crate) fn worker_poll(&mut self, worker: *mut Worker) -> *mut u8 {
        if self.trapped() {
            return std::ptr::null_mut();
        }
        let Some(receive) = self.workers.poll(worker) else {
            self.trap(
                TrapKind::Internal,
                "worker handle is not owned by Context",
                0,
            );
            return std::ptr::null_mut();
        };
        crate::worker::materialize(self, receive)
    }

    pub(crate) fn worker_close(&mut self, worker: *mut Worker) {
        if !self.workers.close(worker) {
            self.trap(
                TrapKind::Internal,
                "worker handle is not owned by Context",
                0,
            );
        }
    }

    pub(crate) fn worker_join(&mut self, worker: *mut Worker) -> bool {
        let Some(outcome) = self.workers.join(worker) else {
            self.trap(
                TrapKind::Internal,
                "worker handle is not owned by Context",
                0,
            );
            return false;
        };
        match outcome {
            WorkerOutcome::Clean => true,
            WorkerOutcome::Trapped(record) => {
                self.trap(
                    TrapKind::WorkerTrapped,
                    format!(
                        "worker trapped with {}: {}",
                        record.kind.rule(),
                        // The parent uses the Worker's trap site.
                        record.message
                    ),
                    record.pos_id,
                );
                false
            }
            WorkerOutcome::ThreadFailed => {
                self.trap(
                    TrapKind::WorkerTrapped,
                    "worker thread ended without a runtime outcome",
                    0,
                );
                false
            }
        }
    }

    /// True while at least one runtime-owned worker thread has not been
    /// joined. Hot reload uses this to keep old generated code live until no
    /// worker can execute it.
    #[must_use]
    pub fn has_live_workers(&self) -> bool {
        self.workers.has_live_workers()
    }

    /// Enables or disables diagnostics for handles to freed allocations.
    ///
    /// When enabled, freed allocations whose requested payload is at least
    /// `min_payload_bytes` are retained within `max_retained_bytes`. Oldest
    /// retained allocations are evicted to make room. Diagnostics are
    /// guaranteed for the most recent covered frees that fit the budget and
    /// best-effort otherwise. Invalid frees remain diagnosed regardless of
    /// the threshold and budget. The setting is disabled by default.
    ///
    /// Returns `false` without changing the setting if an allocation
    /// request has already started. A host must establish the setting
    /// before the first allocation.
    pub fn set_freed_handle_diagnostics(
        &mut self,
        enabled: bool,
        min_payload_bytes: usize,
        max_retained_bytes: usize,
    ) -> bool {
        if self.allocation_started {
            return false;
        }
        self.freed_handle_diagnostics = enabled;
        if enabled {
            self.freed_handle_diagnostics_min_payload_bytes = min_payload_bytes;
            self.freed_handle_diagnostics_max_retained_bytes = max_retained_bytes;
        }
        true
    }

    pub(super) fn uses_ship_arena(&self) -> bool {
        self.ship_arena && !self.freed_handle_diagnostics
    }

    pub(super) fn retains_freed_payload(&self, payload_size: usize) -> bool {
        self.freed_handle_diagnostics
            && payload_size >= self.freed_handle_diagnostics_min_payload_bytes
    }

    /// Byte offset of the trap flag inside the context (ABI contract
    /// with generated code).
    #[must_use]
    pub fn trap_flag_offset() -> usize {
        // repr(C): trap_flag is the first field.
        0
    }

    /// Byte offset of the reload epoch (ABI contract with generated
    /// code lowered in hot-reload mode).
    #[must_use]
    pub fn reload_epoch_offset() -> usize {
        4
    }

    /// Byte offset of the per-function indirection table pointer (ABI
    /// contract with generated code lowered in hot-reload mode).
    #[must_use]
    pub fn fn_table_offset() -> usize {
        8
    }

    /// Byte offset of the module-global block pointer (ABI contract
    /// with generated code lowered in hot-reload mode).
    #[must_use]
    pub fn globals_offset() -> usize {
        16
    }

    // ----- hot-reload state (dev tier) -----

    /// Points the indirection table at `table`, an array of code
    /// addresses indexed by the lowering's function slot numbers.
    ///
    /// Storing the pointer is safe; generated code dereferences it, so
    /// the array must stay alive and correctly sized for as long as
    /// script code compiled in reload mode can run.
    pub fn set_fn_table(&mut self, table: *const *const u8) {
        self.fn_table = table;
    }

    /// Points module-global storage at `base`, a block laid out by the
    /// lowering. It must stay alive for the rest of the session: the
    /// collector's registered root ranges point into it.
    pub fn set_globals(&mut self, base: *mut u8) {
        self.globals = base;
    }

    /// Allocates and installs the ship tier's zeroed module-global block.
    ///
    /// Re-running the same image's initializer on one Context reuses and
    /// zeroes the original block. A different layout on the same Context is
    /// an internal host/codegen mismatch. This structural allocation is not
    /// a language object: it does not consume object fault-injection counts
    /// or participate in collection, and it is freed when the Context drops.
    pub(crate) fn init_module_globals(&mut self, size: usize, align: usize) -> *mut u8 {
        let Ok(layout) = Layout::from_size_align(size.max(1), align) else {
            self.trap(
                TrapKind::Internal,
                "module-global block layout is not representable",
                0,
            );
            return std::ptr::null_mut();
        };
        if let Some(block) = &self.module_globals {
            if block.layout != layout {
                self.trap(
                    TrapKind::Internal,
                    "module-global block layout changed for a live Context",
                    0,
                );
                return std::ptr::null_mut();
            }
            // SAFETY: `base` owns `layout.size()` writable bytes for the
            // lifetime of this Context.
            unsafe { std::ptr::write_bytes(block.base, 0, block.layout.size()) };
            self.globals = block.base;
            return block.base;
        }

        // SAFETY: `layout` is non-empty because `size.max(1)` was used.
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("module-global block allocation of {size} bytes failed"),
                0,
            );
            return std::ptr::null_mut();
        }
        self.globals = base;
        self.module_globals = Some(ModuleGlobals { base, layout });
        base
    }

    /// The current reload epoch. Coroutine frames record the epoch
    /// they were created in; a resume across a swap traps.
    #[must_use]
    pub fn reload_epoch(&self) -> u32 {
        self.reload_epoch
    }

    /// Advances the reload epoch, invalidating every coroutine frame
    /// created before the swap.
    pub fn bump_reload_epoch(&mut self) {
        self.reload_epoch = self.reload_epoch.wrapping_add(1);
    }

    /// Marks entry into script code (the host called into the script).
    pub fn enter_script(&mut self) {
        self.script_depth = self.script_depth.saturating_add(1);
    }

    /// Marks return from script code.
    pub fn exit_script(&mut self) {
        self.script_depth = self.script_depth.saturating_sub(1);
        // compiler.md §115.4 item 1: an exception that leaves the
        // outermost script frame of a host entry becomes a trap.
        if self.script_depth == 0 {
            self.settle_uncaught_exception();
        }
    }

    /// Number of host-to-script calls currently on the stack. A hot
    /// reload is applied only at zero (the frame-boundary rule).
    #[must_use]
    pub fn script_depth(&self) -> u32 {
        self.script_depth
    }

    // ----- poll-driven async roots (Q34) -----

    // ----- Math.random state (stdlib.md §2) -----

    /// Draws the next `Math.random()` value from the Context-owned
    /// xoshiro256++ stream.
    pub fn random_f64(&mut self) -> f64 {
        self.rng.next_f64()
    }

    /// Reseeds the `Math.random` stream by re-expanding `seed` (host
    /// replay control; [`crate::ffi::subscript_rt_ctx_seed_random`]).
    pub fn seed_random(&mut self, seed: u64) {
        self.rng.reseed(seed);
    }

    // ----- Date.now clock (stdlib.md §3) -----

    /// The `Date.now()` source: the pinned value when the host set one
    /// ([`Context::set_now`]), otherwise the system UTC clock in epoch
    /// milliseconds. A pre-1970 system clock yields the exact negative
    /// millisecond value, never a panic.
    #[must_use]
    pub fn now_utc_ms(&self) -> i64 {
        if let Some(ms) = self.now_override {
            return ms;
        }
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_millis() as i64,
            // The clock is before the epoch: the error carries the
            // (positive) distance back to it.
            Err(e) => -(e.duration().as_millis() as i64),
        }
    }

    /// Pins the `Date.now` clock to `ms` (tests, replays;
    /// [`crate::ffi::subscript_rt_ctx_set_now`]). Every later `Date.now()`
    /// returns exactly `ms` until pinned again.
    pub fn set_now(&mut self, ms: i64) {
        self.now_override = Some(ms);
    }

    /// Sets the maximum work allowed for one regular-expression search.
    ///
    /// The value is deterministic Context state. A zero budget is
    /// permitted and causes any nontrivial match attempt to trap.
    pub fn set_regex_budget(&mut self, budget: u64) {
        self.regex_budget = budget;
    }

    /// Current regular-expression execution budget.
    #[must_use]
    pub(crate) fn regex_budget(&self) -> u64 {
        self.regex_budget
    }

    /// Context-owned regular-expression cache and per-handle state.
    pub(crate) fn regex_store(&mut self) -> &mut crate::regexops::RegexStore {
        &mut self.regex
    }

    // ----- trap state -----

    /// Records a trap. The first trap wins; later ones are ignored
    /// (generated code unwinds after the first, but runtime functions
    /// invoked on the unwind path stay callable).
    pub fn trap(&mut self, kind: TrapKind, message: impl Into<String>, pos_id: u32) {
        // compiler.md §115.6 rule 1: a trap drops a pending exception, and
        // the exceptions that wait for their hooks.
        self.pending_exception = None;
        self.parked_exceptions.clear();
        if self.trap.is_none() {
            self.trap = Some(TrapRecord::new(kind, message, pos_id));
            // The observer sees both the stored record and the raised
            // flag. The unconditional write below remains the central
            // unwind-path rule for every call to `trap`.
            self.trap_flag = 1;
            if let Some(observer) = self.trap_observer {
                let record = self.trap.as_ref().expect("trap was just stored");
                let message = record.message.as_bytes();
                let kind = record.kind as u32;
                let pos_id = record.pos_id;
                let message_ptr = if message.is_empty() {
                    std::ptr::null()
                } else {
                    message.as_ptr()
                };
                let message_len = message.len() as u64;
                let userdata = self.trap_observer_userdata;
                // SAFETY: the host supplied the callback and userdata.
                // No Context pointer is passed; the callback contract
                // forbids obtaining and using one by other means while
                // this exclusive borrow is live.
                self.with_trap_observer_active(|| unsafe {
                    observer(userdata, kind, pos_id, message_ptr, message_len);
                });
            }
        }
        // Later faults on the unwind path do not replace the record or
        // notify the observer, but they still re-raise the flag.
        self.trap_flag = 1;
    }

    /// The recorded trap, if any.
    #[must_use]
    pub fn trap_record(&self) -> Option<&TrapRecord> {
        self.trap.as_ref()
    }

    /// Installs a host observer for the first trap in each uncleared
    /// run. Passing `None` clears the observer and its userdata.
    pub fn set_trap_observer(&mut self, observer: Option<TrapObserver>, userdata: *mut c_void) {
        set_observer(
            &mut self.trap_observer,
            &mut self.trap_observer_userdata,
            observer,
            userdata,
        );
    }

    /// Runs `call` while the observer-active guard is raised.
    ///
    /// The drop guard also restores the flag during a Rust unwind. The
    /// public observer type is `extern "C"`, whose callbacks may not
    /// unwind across the ABI boundary; this cleanup additionally covers
    /// every unwind path Rust can construct before such a boundary.
    pub(super) fn with_trap_observer_active(&mut self, call: impl FnOnce()) {
        struct ResetObserverActive<'a>(&'a mut bool);

        impl Drop for ResetObserverActive<'_> {
            fn drop(&mut self) {
                *self.0 = false;
            }
        }

        self.trap_observer_active = true;
        let _reset = ResetObserverActive(&mut self.trap_observer_active);
        call();
    }

    /// Whether trap clearing is legal at the current host boundary.
    #[must_use]
    pub(crate) fn can_clear_trap(&self) -> bool {
        !self.trap_observer_active && self.script_depth == 0
    }

    /// Clears the trap record and lowers the trap flag, so the next
    /// host call into script starts from a clean state
    /// (`specs/blocks/compiler.md` §8.2: a trap does not end the dev
    /// session).
    ///
    /// Clearance also stops the recorded async job, except for reload
    /// staleness (§94.2), and discards unfinished transient JSON builders.
    /// Allocations, globals, roots, the stdout sink, and the
    /// reload epoch are all untouched, so nothing a trap protected
    /// against becomes reachable again: a deleted allocation stays
    /// poisoned and a stale coroutine stays stale (its frame epoch still
    /// differs, so resuming it traps again).
    ///
    /// The caller must be at a host↔script boundary — no generated
    /// code on the stack. Generated code reads the flag after every
    /// fault-capable call and unwinds on it; clearing while a script
    /// frame is live would resume a run that has already given up.
    /// [`Context::script_depth`] is the check.
    pub fn clear_trap(&mut self) {
        if let Some((job, kind)) = self.async_trapping.take() {
            if kind != TrapKind::StaleCoroutine {
                self.async_ready.retain(|queued| *queued != job);
                self.async_clear_trapping_job(job);
            }
        }
        self.trap = None;
        self.pending_exception = None;
        self.parked_exceptions.clear();
        self.trap_flag = 0;
        // A trapping JSON operation may unwind before its finish leaf on
        // the dev tier. Builders and parsed trees are transient
        // implementation state, not language-visible state.
        self.json_builders.clear();
        self.json_parsers.clear();
    }

    // ----- JSON.stringify transient builders (stdlib.md §13) -----

    pub(crate) fn json_builders(&mut self) -> &mut crate::json::JsonBuilders {
        &mut self.json_builders
    }

    pub(crate) fn json_parsers(&mut self) -> &mut crate::json::JsonParsers {
        &mut self.json_parsers
    }

    /// True when a trap is pending.
    #[must_use]
    pub fn trapped(&self) -> bool {
        self.trap_flag != 0
    }

    // ----- stdout sink -----

    /// Delivers `bytes` to the installed print observer without retaining
    /// them. With no observer, appends `bytes` and a newline to the
    /// stdout sink.
    pub fn print_line(&mut self, bytes: &[u8]) {
        // SAFETY: `bytes` is a live slice for this call, and the sink is
        // a separate allocation, so the append does not move it.
        unsafe { self.print_view(bytes.as_ptr(), bytes.len()) };
    }

    /// The same as [`Context::print_line`], over the bytes of a live
    /// string handle, with no copy of the line.
    ///
    /// # Safety
    ///
    /// `handle` must be a live string handle of this Context.
    pub unsafe fn print_str(&mut self, handle: *const u8) {
        // SAFETY: the caller guarantees a live handle; the view outlives
        // this call because the sink is a separate allocation and
        // nothing here deletes or collects.
        let bytes: &[u8] = unsafe { self.str_view(handle) };
        // SAFETY: the view is readable for this call.
        unsafe { self.print_view(bytes.as_ptr(), bytes.len()) };
    }

    /// Delivers `len` bytes at `ptr`, or appends them and a newline to
    /// the stdout sink.
    ///
    /// # Safety
    ///
    /// `ptr` must point at `len` readable bytes that this call does not
    /// move: either the caller's own memory, or a Context string
    /// allocation, which the append leaves in place.
    pub(super) unsafe fn print_view(&mut self, ptr: *const u8, len: usize) {
        if let Some(observer) = self.print_observer {
            let userdata = self.print_observer_userdata;
            // SAFETY: the host supplied the callback and userdata. The
            // callback contract forbids obtaining and using this Context
            // while the exclusive borrow is live. An observed line is not
            // retained.
            unsafe { observer(userdata, ptr, len as u64) };
            return;
        }
        // SAFETY: the caller guarantees `len` readable bytes at `ptr`
        // that this append does not move.
        let line = unsafe { std::slice::from_raw_parts(ptr, len) };
        self.stdout.extend_from_slice(line);
        self.stdout.push(b'\n');
    }

    /// Installs a host observer for printed lines. Passing `None` clears
    /// the observer and its userdata.
    pub fn set_print_observer(&mut self, observer: Option<PrintObserver>, userdata: *mut c_void) {
        set_observer(
            &mut self.print_observer,
            &mut self.print_observer_userdata,
            observer,
            userdata,
        );
    }

    /// Installs the host observer for optional runtime diagnostics
    /// advisories. Passing `None` clears the observer and its userdata.
    pub fn set_diagnostics_observer(
        &mut self,
        observer: Option<DiagnosticsObserver>,
        userdata: *mut c_void,
    ) {
        set_observer(
            &mut self.diagnostics_observer,
            &mut self.diagnostics_observer_userdata,
            observer,
            userdata,
        );
    }

    /// Sets the callback-binding count at which newly interned records are
    /// reported through the optional diagnostics observer.
    ///
    /// The threshold is literal: zero advises on the first record. The
    /// default is [`u64::MAX`].
    pub fn set_binding_count_advisory(&mut self, threshold: u64) {
        self.binding_count_advisory_threshold = threshold;
    }

    /// Reports a newly interned callback binding, or a new §111
    /// registration, when the resulting count is at or above the
    /// configured threshold.
    ///
    /// The count is the binding records plus the live registrations
    /// (§111 rule 9).
    pub(crate) fn advise_binding_count(&mut self) {
        let records = self
            .callbacks
            .len()
            .saturating_add(self.registrations.len());
        let count = u64::try_from(records).unwrap_or(u64::MAX);
        let threshold = self.binding_count_advisory_threshold;
        if count < threshold {
            return;
        }
        let Some(observer) = self.diagnostics_observer else {
            return;
        };

        let message =
            format!("callback bindings: {count} registered, advisory threshold {threshold}");
        let userdata = self.diagnostics_observer_userdata;
        // SAFETY: the host supplied the callback and userdata. No Context
        // pointer is passed, and the callback contract forbids recovering
        // and using this exclusively borrowed Context by other means.
        unsafe {
            observer(
                userdata,
                DIAGNOSTICS_ADVISORY_BINDING_COUNT,
                0,
                message.as_ptr(),
                message.len() as u64,
            )
        };
    }

    /// Reports an explicit free of registered callback userdata when the
    /// optional diagnostics observer is installed.
    ///
    /// The scan covers the binding records and the live §111
    /// registrations (§111 rule 9). The observer-none branch returns
    /// before it, so the default path pays neither the scan nor any
    /// retained state.
    pub(super) fn advise_callback_userdata_free(&mut self, payload: usize, pos_id: u32) {
        let Some(observer) = self.diagnostics_observer else {
            return;
        };
        if !self.is_live(payload) {
            return;
        }
        let held = self.callbacks.iter().any(|binding| {
            binding.userdata1 as usize == payload || binding.userdata2 as usize == payload
        }) || self.registration_holds_userdata(payload);
        if !held {
            return;
        }

        const MESSAGE: &[u8] = b"Context.free of registered callback userdata";
        let userdata = self.diagnostics_observer_userdata;
        // SAFETY: the host supplied the callback and userdata. No Context
        // pointer is passed, and the callback contract forbids recovering
        // and using this exclusively borrowed Context by other means.
        unsafe {
            observer(
                userdata,
                DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE,
                pos_id,
                MESSAGE.as_ptr(),
                MESSAGE.len() as u64,
            )
        };
    }

    /// Takes the captured stdout bytes.
    #[must_use]
    pub fn take_stdout(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stdout)
    }

    /// Borrows the captured stdout bytes without draining the sink.
    #[must_use]
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout
    }

    // ----- allocation -----

    /// Refuses the `n`-th subsequent object-level allocation request.
    ///
    /// `n == 0` disables a pending fault. A fired fault is one-shot.
    pub fn fail_alloc_after(&mut self, n: u64) {
        self.alloc_fail_countdown = (n != 0).then_some(n);
    }

    /// Allocates one zeroed system block and initializes its header.
    pub(super) fn alloc_system_block(
        &mut self,
        size: usize,
        class_id: u32,
        pos_id: u32,
    ) -> Option<(*mut u8, Layout)> {
        let total = HEADER_SIZE.saturating_add(size.max(1));
        let Ok(layout) = Layout::from_size_align(total, 16) else {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes is not representable"),
                pos_id,
            );
            return None;
        };
        // SAFETY: `layout` has non-zero size (>= HEADER_SIZE + 1).
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes failed"),
                pos_id,
            );
            return None;
        }
        // SAFETY: `base` is a fresh allocation with a complete header.
        unsafe { write_header(base, class_id, pos_id) };
        Some((base, layout))
    }

    /// Allocates `size` payload bytes tagged `class_id`.
    ///
    /// Returns the payload pointer, or null after an allocation trap.
    /// The development tier returns zeroed bytes. The ship tier also
    /// zeroes fresh storage and reused classes that can hold handles.
    /// A handle-free class must replace all exposed bytes before a read.
    /// [`Context::alloc_str_with`] supplies that writer for strings.
    pub fn alloc(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
        self.allocation_started = true;
        if let Some(remaining) = self.alloc_fail_countdown {
            if remaining == 1 {
                self.alloc_fail_countdown = None;
                self.trap(
                    TrapKind::AllocationFailure,
                    "injected allocation failure",
                    pos_id,
                );
                return std::ptr::null_mut();
            }
            self.alloc_fail_countdown = Some(remaining - 1);
        }
        if self.uses_ship_arena() {
            return self.arena_alloc(size, class_id, pos_id);
        }
        let Some((base, layout)) = self.alloc_system_block(size, class_id, pos_id) else {
            return std::ptr::null_mut();
        };
        // SAFETY: the system block includes a complete header.
        let payload = unsafe { base.add(HEADER_SIZE) };
        self.allocations.insert(
            payload as usize,
            Allocation {
                base,
                layout,
                payload_size: size,
                class_id,
                marked: false,
            },
        );
        self.live_bytes_counter = self.live_bytes_counter.saturating_add(size);
        payload
    }

    /// Starts a nested call-duration boundary-scratch scope.
    #[must_use]
    pub fn boundary_scratch_mark(&self) -> usize {
        self.boundary_scratch.len()
    }

    /// Allocates zeroed, 16-aligned storage outside the managed heap for a
    /// recursively lowered C element array or pointed-to struct. The matching
    /// mark release frees it after the synchronous foreign call returns.
    pub fn boundary_scratch_alloc(&mut self, size: usize, pos_id: u32) -> *mut u8 {
        let Ok(layout) = Layout::from_size_align(size.max(1), 16) else {
            self.trap(
                TrapKind::AllocationFailure,
                format!("boundary scratch allocation of {size} bytes is not representable"),
                pos_id,
            );
            return std::ptr::null_mut();
        };
        // SAFETY: `layout` is non-empty and has a supported power-of-two
        // alignment.
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("boundary scratch allocation of {size} bytes failed"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        self.boundary_scratch
            .push(BoundaryScratchAllocation { base, layout });
        base
    }

    /// Releases the suffix allocated since `mark`. An out-of-range mark is
    /// an internal ABI disagreement; release everything rather than leak.
    pub fn boundary_scratch_release(&mut self, mark: usize) {
        let mark = if mark <= self.boundary_scratch.len() {
            mark
        } else {
            0
        };
        for allocation in self.boundary_scratch.drain(mark..).rev() {
            // SAFETY: each record owns a distinct allocation made by
            // `boundary_scratch_alloc` and is drained exactly once.
            unsafe { dealloc(allocation.base, allocation.layout) };
        }
    }

    // ----- ship-tier arena internals (§8.1b) -----

    /// Size-class index for a total block size `need`
    /// (`need <= LARGEST_BLOCK`): the smallest power-of-two block that
    /// holds it, `SMALLEST_BLOCK` at minimum.
    #[inline]
    pub(super) fn size_class(need: usize) -> usize {
        let rounded = need.next_power_of_two().max(SMALLEST_BLOCK);
        (rounded.trailing_zeros() - SMALLEST_BLOCK.trailing_zeros()) as usize
    }

    /// Ship-tier `alloc`: free-list pop, else bump from the class's open
    /// chunk, else a new chunk; above the largest class, an individual
    /// system allocation with a `LargeAlloc` record. A reused payload is
    /// zeroed when its class can hold handles. A string writer replaces
    /// all exposed bytes. Fresh chunks and large allocations stay zeroed.
    pub(super) fn arena_alloc(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
        let need = HEADER_SIZE.saturating_add(size.max(1));
        if need > LARGEST_BLOCK {
            return self.arena_alloc_large(size, class_id, pos_id);
        }
        let class = Self::size_class(need);
        let block_size = SMALLEST_BLOCK << class;

        let head = self.free_heads[class];
        if head != 0 {
            let payload = head as *mut u8;
            // SAFETY: `head` is a payload address this arena free-listed:
            // its block (header + `block_size - HEADER_SIZE` payload
            // capacity) is inside an owned chunk. The next link occupies
            // the payload's first word. After unlinking, classes that can
            // hold handles are re-zeroed before the header is re-armed.
            unsafe {
                self.free_heads[class] = (payload as *const usize).read();
                if !class_holds_no_handle(class_id) {
                    std::ptr::write_bytes(payload, 0, block_size - HEADER_SIZE);
                }
                let base = payload.sub(HEADER_SIZE);
                write_header(base, class_id, pos_id);
            }
            self.live_bytes_counter = self
                .live_bytes_counter
                .saturating_add(block_size - HEADER_SIZE);
            return payload;
        }

        let blocks_per_chunk = CHUNK_SIZE / block_size;
        let ci = match self.open[class] {
            Some(i) if self.chunks[i].bump < blocks_per_chunk => i,
            _ => match self.arena_new_chunk(class, pos_id) {
                Some(i) => i,
                None => return std::ptr::null_mut(),
            },
        };
        let chunk = &mut self.chunks[ci];
        // SAFETY: `bump < blocks_per_chunk`, so the block lies inside the
        // chunk. The chunk came from `alloc_zeroed` and this block was
        // never handed out, so its payload is already zero; only the
        // header needs writing.
        let payload = unsafe {
            let base = chunk.base.add(chunk.bump * block_size);
            write_header(base, class_id, pos_id);
            base.add(HEADER_SIZE)
        };
        chunk.bump += 1;
        self.live_bytes_counter = self
            .live_bytes_counter
            .saturating_add(block_size - HEADER_SIZE);
        payload
    }

    /// Allocates and registers a fresh chunk for `class`; returns its
    /// index in `chunks`, or `None` after an allocation-failure trap.
    pub(super) fn arena_new_chunk(&mut self, class: usize, pos_id: u32) -> Option<usize> {
        let Ok(layout) = Layout::from_size_align(CHUNK_SIZE, 16) else {
            self.trap(
                TrapKind::AllocationFailure,
                "arena chunk layout is not representable",
                pos_id,
            );
            return None;
        };
        // SAFETY: `layout` has non-zero size (CHUNK_SIZE).
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("arena chunk allocation of {CHUNK_SIZE} bytes failed"),
                pos_id,
            );
            return None;
        }
        let idx = self.chunks.len();
        self.chunks.push(Chunk {
            base,
            layout,
            block_size: SMALLEST_BLOCK << class,
            class,
            bump: 0,
        });
        // Keep the membership index sorted by base address.
        let pos = self.chunk_map.partition_point(|&(b, _)| b < base as usize);
        self.chunk_map.insert(pos, (base as usize, idx));
        self.open[class] = Some(idx);
        #[cfg(test)]
        self.stats
            .chunks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Some(idx)
    }

    /// Large-allocation path: an individual system allocation recorded in
    /// `large`. The header carries state, class id, and allocating position
    /// id; the payload size lives in the record (a large payload may exceed
    /// `u32`).
    pub(super) fn arena_alloc_large(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
        let Some((base, layout)) = self.alloc_system_block(size, class_id, pos_id) else {
            return std::ptr::null_mut();
        };
        // SAFETY: the system block includes a complete header.
        let payload = unsafe { base.add(HEADER_SIZE) };
        self.large.insert(
            payload as usize,
            LargeAlloc {
                base,
                layout,
                payload_size: size,
            },
        );
        self.live_bytes_counter = self.live_bytes_counter.saturating_add(size);
        #[cfg(test)]
        self.stats
            .large
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        payload
    }

    /// The exact membership test's chunk half (§8.1b): `addr` is a
    /// classed block's payload only if it falls inside a known chunk, on
    /// that chunk's block grid, below the chunk's bump watermark. Returns
    /// the block base and size class; the caller still checks the header
    /// state (the fourth condition) — a hit here may be a free-listed
    /// (dead) block.
    pub(super) fn arena_lookup_block(&self, addr: usize) -> Option<(*mut u8, usize)> {
        #[cfg(test)]
        self.stats
            .membership_lookups
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let i = self.chunk_map.partition_point(|&(b, _)| b <= addr);
        if i == 0 {
            return None;
        }
        let (cbase, ci) = self.chunk_map[i - 1];
        if addr < cbase + HEADER_SIZE || addr >= cbase + CHUNK_SIZE {
            return None;
        }
        let chunk = &self.chunks[ci];
        let off = addr - cbase - HEADER_SIZE;
        // Block sizes are powers of two: mask/shift are the grid checks.
        if off & (chunk.block_size - 1) != 0 {
            return None;
        }
        let bi = off >> chunk.block_size.trailing_zeros();
        if bi >= chunk.bump {
            return None;
        }
        // SAFETY: `bi < bump <= blocks_per_chunk`, so the block base is
        // inside the owned chunk.
        Some((
            unsafe { chunk.base.add(bi * chunk.block_size) },
            chunk.class,
        ))
    }

    /// Clears the separately allocated storage owned by a Map/Set before
    /// its header is retired.
    ///
    /// `payload` must be a live Map/Set header owned by this Context.
    pub(super) fn clear_container_on_delete(&mut self, payload: usize) {
        #[cfg(test)]
        self.stats
            .container_delete_entries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // SAFETY: callers reach this helper only after exact membership,
        // liveness, and Map/Set class-id checks.
        unsafe { crate::assocops::clear(self, payload as *mut u8) };
    }

    /// Removes Context-lifetime string-intern entries before their ordinary
    /// allocation is retired through the exported delete path.
    pub(super) fn clear_string_interns_on_delete(&mut self, payload: usize) {
        self.interned.retain(|_, handle| *handle != payload);
        self.astral_code_points
            .retain(|_, handle| *handle != payload);
    }

    /// Releases the Context-owned state for one live classed allocation.
    ///
    /// The caller must keep the allocation header live until this function
    /// returns. Map and Set release work reads that header and recursively
    /// retires its separate storage.
    pub(super) fn release_class_state(&mut self, class_id: u32, payload: usize) {
        match class_id {
            CLASS_MAP | CLASS_SET => self.clear_container_on_delete(payload),
            CLASS_STRING => self.clear_string_interns_on_delete(payload),
            CLASS_REGEX => self.regex.remove_value(payload),
            _ => {}
        }
    }

    /// Ship-tier release: a live classed block goes to its class's free
    /// list; a large record is freed and dropped. Map/Set storage is
    /// cleared after the same membership/header read that release already
    /// requires, avoiding a container-specific lookup before every
    /// ordinary delete. Anything else — dead, free-listed, or unknown —
    /// is undefined per Q6/§8.1b and handled as a no-op (never a trap,
    /// never relied upon).
    pub(super) fn arena_release(&mut self, payload: usize) {
        if let Some((block, class)) = self.arena_lookup_block(payload) {
            // SAFETY: `block` heads a block inside an owned chunk; both
            // header reads stay inside it.
            let class_id = unsafe {
                if !matches!((block as *const u64).read(), LIVE_STATE | MARK_STATE) {
                    return;
                }
                header_class_id(block)
            };
            self.release_class_state(class_id, payload);
            // SAFETY: the live-grid check above proves `block` is still
            // owned. Container clearing only retires its separate child
            // allocations, so the state word and payload free-list link
            // remain valid.
            unsafe {
                (block as *mut u64).write(DEAD_STATE);
                (payload as *mut usize).write(self.free_heads[class]);
                self.free_heads[class] = payload;
            }
            self.live_bytes_counter = self
                .live_bytes_counter
                .saturating_sub((SMALLEST_BLOCK << class) - HEADER_SIZE);
            return;
        }
        if let Some(a) = self.large.remove(&payload) {
            // SAFETY: removing the exact live record proves `a.base`
            // heads an owned allocation whose class-id word is readable.
            let class_id = unsafe { header_class_id(a.base) };
            self.release_class_state(class_id, payload);
            self.live_bytes_counter = self.live_bytes_counter.saturating_sub(a.payload_size);
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `arena_alloc_large`; the record was just removed so this
            // frees it exactly once. Container clearing only retired
            // separate child allocations.
            unsafe { dealloc(a.base, a.layout) };
            #[cfg(test)]
            self.stats
                .large
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Moves one live exact-size allocation into retained-dead storage when
    /// its layout fits the retention budget. Oldest records are evicted until
    /// the new record fits; an individually over-budget layout is released.
    pub(super) fn retire_dev_allocation(&mut self, payload: usize) -> Option<u32> {
        let allocation = self.allocations.remove(&payload)?;
        self.live_bytes_counter = self
            .live_bytes_counter
            .saturating_sub(allocation.payload_size);
        // SAFETY: exact live-map membership proves the complete initialized
        // header remains owned by this Context.
        let class_id = allocation.class_id;
        Self::retain_or_release_removed_allocation(
            &mut self.dead_allocations,
            &mut self.retained_allocations,
            &mut self.retained_bytes,
            self.freed_handle_diagnostics_max_retained_bytes,
            payload,
            allocation,
        );
        Some(class_id)
    }

    /// Retains one removed exact-size allocation within the byte budget, or
    /// releases it when its layout alone exceeds the budget.
    pub(super) fn retain_or_release_removed_allocation(
        dead_allocations: &mut AddressSet,
        retained_allocations: &mut VecDeque<RetainedAllocation>,
        retained_bytes: &mut usize,
        max_retained_bytes: usize,
        payload: usize,
        allocation: Allocation,
    ) {
        let layout_bytes = allocation.layout.size();
        if layout_bytes > max_retained_bytes {
            // SAFETY: the live record was removed by the caller, so this
            // allocation is released exactly once.
            unsafe { dealloc(allocation.base, allocation.layout) };
            return;
        }

        while *retained_bytes > max_retained_bytes - layout_bytes {
            let oldest = retained_allocations
                .pop_front()
                .expect("retained byte accounting requires an oldest allocation");
            let removed = dead_allocations.remove(&oldest.payload);
            debug_assert!(
                removed,
                "retained ownership queue and dead address set must agree"
            );
            *retained_bytes = retained_bytes
                .checked_sub(oldest.layout.size())
                .expect("retained byte accounting cannot underflow");
            // SAFETY: popping the ownership record makes this the allocation's
            // unique release. Its address was removed from the dead set first.
            unsafe { dealloc(oldest.base, oldest.layout) };
        }

        // SAFETY: the retained allocation owns at least HEADER_SIZE bytes;
        // poisoning preserves the diagnostic-mode stale-handle trap.
        unsafe { (allocation.base as *mut u64).write(DEAD_STATE) };
        let inserted = dead_allocations.insert(payload);
        // Until budget eviction, this owned backing allocation is unavailable
        // for allocator reuse and remains disjoint from the live map.
        debug_assert!(
            inserted,
            "live allocation map and retained-dead address set must be disjoint"
        );
        *retained_bytes += layout_bytes;
        retained_allocations.push_back(RetainedAllocation {
            payload,
            base: allocation.base,
            layout: allocation.layout,
        });
        debug_assert!(*retained_bytes <= max_retained_bytes);
        debug_assert_eq!(dead_allocations.len(), retained_allocations.len());
    }

    /// Releases one exact-size allocation and removes its live record.
    pub(super) fn release_dev_allocation(&mut self, payload: usize) -> Option<u32> {
        let allocation = self.allocations.remove(&payload)?;
        self.live_bytes_counter = self
            .live_bytes_counter
            .saturating_sub(allocation.payload_size);
        // SAFETY: exact live-map membership proves the complete initialized
        // header remains owned by this Context.
        let class_id = allocation.class_id;
        // SAFETY: `base`/`layout` came from `alloc_zeroed` in `alloc`; the
        // live record was just removed, so this frees it exactly once.
        unsafe { dealloc(allocation.base, allocation.layout) };
        Some(class_id)
    }

    /// Retains or releases one exact-size allocation according to the
    /// diagnostics mode's requested-payload threshold.
    pub(super) fn dispose_dev_allocation(&mut self, payload: usize) -> Option<u32> {
        let retain = self
            .allocations
            .get(&payload)
            .is_some_and(|allocation| self.retains_freed_payload(allocation.payload_size));
        if retain {
            self.retire_dev_allocation(payload)
        } else {
            self.release_dev_allocation(payload)
        }
    }

    /// Frees or marks the allocation at `payload` dead, per Context policy.
    ///
    /// With freed-handle diagnostics enabled, allocations at or above the
    /// configured payload threshold are retained and poisoned within the
    /// byte budget, evicting oldest records first. Stale-handle,
    /// double-delete, and unknown-pointer checks remain enabled for every
    /// payload size (Q6/§8.1a-3).
    ///
    /// With diagnostics disabled, the backing allocation is released.
    /// A double delete or unknown pointer is undefined (Q6/§8.1b) and
    /// handled as a no-op (no trap).
    pub fn delete(&mut self, payload: usize, pos_id: u32) {
        self.advise_callback_userdata_free(payload, pos_id);
        if !self.object_descriptions.is_empty() || !self.counted_maps.is_empty() {
            self.release_reference_holder(payload, pos_id);
        }
        if self.uses_ship_arena() {
            self.arena_release(payload);
            return;
        }
        let Some(allocation) = self.allocations.get(&payload) else {
            if self.freed_handle_diagnostics {
                if self.dead_allocations.contains(&payload) {
                    self.trap(
                        TrapKind::DoubleDelete,
                        TrapKind::DoubleDelete.message(None),
                        pos_id,
                    );
                } else {
                    self.trap(
                        TrapKind::InvalidDelete,
                        "Context.free of a pointer the Context does not own",
                        pos_id,
                    );
                }
            }
            return;
        };
        // SAFETY: exact live-map membership proves the header is readable.
        let class_id = unsafe { header_class_id(allocation.base) };
        // End the allocation-table borrow before release work. Map and Set
        // storage retires through recursive `delete` calls.
        self.release_class_state(class_id, payload);
        let _ = self.dispose_dev_allocation(payload);
    }

    /// True when `payload` is a live allocation (test/inspection aid).
    /// Ship tier: the exact membership test — chunk range, block grid,
    /// bump watermark, live header — or a large record (§8.1b).
    #[must_use]
    pub fn is_live(&self, payload: usize) -> bool {
        if self.uses_ship_arena() {
            if let Some((block, _)) = self.arena_lookup_block(payload) {
                // SAFETY: `block` heads a block inside an owned chunk.
                return matches!(
                    unsafe { (block as *const u64).read() },
                    LIVE_STATE | MARK_STATE
                );
            }
            return self.large.contains_key(&payload);
        }
        self.allocations.contains_key(&payload)
    }

    /// Validates a runtime-operation receiver under Q6's Context policy.
    ///
    /// Freed-handle diagnostics validate exact allocation membership and
    /// trap stale handles. With diagnostics off, use-after-delete is
    /// undefined, so the runtime preserves its unchecked behavior.
    pub(crate) fn require_live_handle(&mut self, payload: usize, pos_id: u32) -> bool {
        if self.trapped() {
            return false;
        }
        if !self.freed_handle_diagnostics || self.is_live(payload) {
            return true;
        }
        self.trap(
            TrapKind::UseAfterDelete,
            TrapKind::UseAfterDelete.message(None),
            pos_id,
        );
        false
    }

    /// Validates one non-null callback userdata slot immediately before a
    /// callback enters script code.
    ///
    /// Diagnostic retained-dead membership is checked first so a retained
    /// header can attribute the trap to the freed allocation. Every other
    /// absent address receives the best-effort liveness diagnostic.
    ///
    /// §14.4b (A), kept by §112 rule 5: the position is the site of the
    /// freed allocation when the runtime holds it, and the reserved
    /// entry of §112 rule 1 when it does not.
    pub(crate) fn validate_callback_userdata(&mut self, payload: *mut u8) -> bool {
        if payload.is_null() {
            return true;
        }
        let address = payload as usize;
        if self.freed_handle_diagnostics && self.dead_allocations.contains(&address) {
            // SAFETY: dead-set membership means the retained allocation and
            // its complete header are still owned by this Context.
            let pos_id = unsafe { payload.offset(POS_ID_OFFSET as isize).cast::<u32>().read() };
            self.trap(
                TrapKind::CallbackUserdataFreed,
                "callback userdata points to a freed allocation",
                pos_id,
            );
            return false;
        }
        if self.is_live(address) {
            return true;
        }
        // No script site exists here: the runtime holds no header for
        // this address, so it holds no position either (§112 rule 5).
        // Id 0 is the reserved entry, so every tier reports the empty
        // position.
        self.trap(
            TrapKind::CallbackUserdataFreed,
            "callback userdata is not a live allocation",
            0,
        );
        false
    }
}
