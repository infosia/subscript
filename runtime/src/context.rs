//! The Context: owner of every script allocation (reference-class
//! instances, array storage, string storage, coroutine frames), the
//! stdout sink, the GC roots the generated code registers, and the
//! trap state.
//!
//! # Memory model
//!
//! Every allocation is `HEADER_SIZE` bytes of header followed by the
//! payload; handles held by script code are payload pointers. The
//! header holds a state word (`LIVE_STATE` / `DEAD_STATE`), the class
//! id, and the allocating source-position id; generated code reads the
//! first two directly (use-after-delete checks, checked `as` narrowing),
//! so their offsets are part of the runtime's ABI contract.
//!
//! Development-tier policy: `unsafeDelete` and `collect()` mark an
//! allocation dead and poison its header but keep the bytes until the
//! Context is dropped. This is what makes double delete and
//! use-after-delete *trap* instead of being undefined: a stale handle
//! still points at owned memory whose header says `DEAD_STATE`.
//!
//! Ship-tier policy (§8.1b): no per-allocation map. Blocks up to the
//! largest size class are carved from Context-owned per-class chunks by
//! bump pointer; `unsafeDelete` pushes a block onto its class's LIFO
//! free list (threaded through the freed payload's first word) and the
//! next same-class `alloc` pops it, zeroed. Larger allocations are
//! individual system allocations with their own record. Double delete
//! and use-after-delete are undefined here (Q6, trusted scripts); the
//! dev tier is the diagnosing tier.
//!
//! # Collection
//!
//! `collect()` never runs unbidden (design invariant 2). Roots are the
//! addresses generated code registers: module-global slots
//! (`root_add`) and per-call shadow frames of managed locals
//! (`shadow_push`/`shadow_pop`). Marking is conservative: the payload
//! of every reached allocation is scanned for pointer-aligned words
//! that equal a live payload address (this covers reference-class
//! fields, array elements, array data pointers, and coroutine frame
//! slots without layout metadata). Conservative marking can retain
//! garbage; it never frees a reachable allocation.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashMap;
use std::ffi::c_void;

use crate::trap::{TrapKind, TrapRecord};

/// Host callback invoked when a Context records its first trap.
///
/// The callback deliberately receives no [`Context`] handle. It runs
/// inside [`Context::trap`] while that method holds exclusive access to
/// the Context, so calling any `sub_rt_*` API that takes that Context
/// (including through a pointer smuggled in `userdata`) would violate
/// Rust's aliasing rules and is undefined behaviour.
///
/// `message` points into the stored [`TrapRecord`]. The bytes remain
/// valid after the callback returns, until the trap is cleared or the
/// Context is released.
pub type TrapObserver = unsafe extern "C" fn(
    userdata: *mut c_void,
    kind: u32,
    pos_id: u32,
    message: *const u8,
    message_len: u64,
);

/// Host callback invoked once for each live Context allocation.
///
/// `payload_bytes` follows the same tier policy as
/// [`Context::live_bytes`]: exact requested bytes in the development
/// tier and for ship-tier large allocations, size-class payload capacity
/// for ship-tier arena blocks.
pub type AllocationVisitor = unsafe extern "C" fn(
    userdata: *mut c_void,
    class_id: u32,
    pos_id: u32,
    payload_bytes: u64,
);

/// C calling convention shared by the module initializer (`ss_init`)
/// and the required exported `main(): void` (`ss_export_main`).
///
/// A host that may clear traps brackets each call with
/// `sub_rt_ctx_enter_script` and `sub_rt_ctx_exit_script`.
///
/// Every other currently supported host export is a zero-argument
/// `void` script function using the symbol `ss_export_<name>` and this
/// same C signature.
pub type ScriptMainEntry = unsafe extern "C" fn(ctx: *mut Context);

/// Bytes between an allocation's base and its payload.
pub const HEADER_SIZE: usize = 16;
/// Header state word for a live allocation (offset -16 from payload).
pub const LIVE_STATE: u64 = 0x5355_4253_4C49_5645; // "SUBSLIVE"
/// Header state word for a deleted/collected allocation.
pub const DEAD_STATE: u64 = 0x5355_4253_4445_4144; // "SUBSDEAD"
/// Byte offset of the state word relative to the payload pointer.
pub const STATE_OFFSET: i32 = -16;
/// Byte offset of the class id relative to the payload pointer.
pub const CLASS_ID_OFFSET: i32 = -8;
/// Byte offset of the allocating position id relative to the payload pointer.
pub const POS_ID_OFFSET: i32 = -4;

/// Class id used for string allocations.
pub const CLASS_STRING: u32 = 0xFFFF_FF01;
/// Class id used for dynamic-array headers.
pub const CLASS_ARRAY: u32 = 0xFFFF_FF02;
/// Class id used for dynamic-array element storage.
pub const CLASS_ARRAY_DATA: u32 = 0xFFFF_FF03;
/// Class id used for coroutine frames.
pub const CLASS_GENERATOR: u32 = 0xFFFF_FF04;
/// Class id used for `Map` headers.
pub const CLASS_MAP: u32 = 0xFFFF_FF05;
/// Class id used for `Set` headers.
pub const CLASS_SET: u32 = 0xFFFF_FF06;
/// Class id used for insertion-ordered Map/Set entry storage.
pub const CLASS_MAP_DATA: u32 = 0xFFFF_FF07;
/// Class id used for Map/Set open-addressed bucket storage.
pub const CLASS_MAP_INDEX: u32 = 0xFFFF_FF08;

// ----- ship-tier arena (§8.1b) -----

// Header state word for a block reached by the current `collect()` mark
// phase (ship tier only). Lives only between mark and sweep — no script
// code runs during `collect`, and sweep restores survivors to
// `LIVE_STATE` — so generated code never observes it.
const MARK_STATE: u64 = 0x5355_4253_4D41_524B; // "SUBSMARK"

// Total block size (header + payload capacity) of the smallest size
// class: fits the 16-byte header plus a 16-byte payload (the `tree`
// benchmark's node) exactly.
const SMALLEST_BLOCK: usize = 32;
// Total block size of the largest class; anything needing more is an
// individual system allocation with a `LargeAlloc` record.
const LARGEST_BLOCK: usize = 4096;
// Classes are power-of-two block sizes: 32, 64, ..., 4096.
const NUM_CLASSES: usize =
    (LARGEST_BLOCK.trailing_zeros() - SMALLEST_BLOCK.trailing_zeros() + 1) as usize;
// Bytes per chunk; every chunk serves a single size class, so its block
// grid is uniform and membership is computable (§8.1b).
const CHUNK_SIZE: usize = 64 * 1024;

/// One ship-tier arena chunk: a system allocation carved into
/// equal-size blocks of one size class. `base` is 16-aligned and every
/// block size is a multiple of 16, so payloads keep 16-byte alignment.
struct Chunk {
    base: *mut u8,
    layout: Layout,
    /// Total block size (header + payload capacity); the grid pitch.
    block_size: usize,
    /// Size-class index (selects the free list).
    class: usize,
    /// Blocks handed out so far; the membership/sweep watermark.
    bump: usize,
}

/// A ship-tier allocation above `LARGEST_BLOCK` (§8.1b): an individual
/// system allocation, keyed by payload address in `Context::large` so
/// it stays enumerable. Presence in the map means live (records are
/// removed when freed); the block still carries the 16-byte header for
/// generated code and for the collect mark state.
struct LargeAlloc {
    base: *mut u8,
    layout: Layout,
    payload_size: usize,
}

/// Test-only balance of arena resources a Context holds, shared out via
/// `Arc` so a test can observe that `Drop` released everything.
#[cfg(test)]
#[derive(Default)]
struct ArenaStats {
    chunks: std::sync::atomic::AtomicUsize,
    large: std::sync::atomic::AtomicUsize,
    membership_lookups: std::sync::atomic::AtomicUsize,
    container_delete_entries: std::sync::atomic::AtomicUsize,
}

/// Payload layout of a dynamic array (Q4): length, capacity, element
/// size, and a pointer to a separate `CLASS_ARRAY_DATA` allocation.
#[repr(C)]
struct ArrayHeader {
    len: u64,
    cap: u64,
    elem_size: u64,
    data: *mut u8,
}

struct Allocation {
    base: *mut u8,
    layout: Layout,
    payload_size: usize,
    live: bool,
    marked: bool,
}

/// A registered C-callback binding (P5.2b). The language's function value
/// is a `(code, env)` pair with the calling convention `(ctx, env,
/// args...)`; a C callback wants a bare `(fnptr, void* userdata)`. A
/// generic C-ABI trampoline ([`crate::ffi::sub_rt_cb_trampoline`]) bridges
/// the two: the record is what the trampoline receives through the C
/// `userdata` slot, so it carries everything the language convention needs
/// — the Context, the language `code`/`env`, and the *real* userdata the
/// script registered. Records live for the whole Context (the Q13
/// lifetime rule: userdata must outlive the registration that holds it).
///
/// P7.2 (§14.4): the record carries **two** userdata slots (`userdata1`,
/// `userdata2`), both delivered to the language callback. A callback-info
/// with one userdata slot binds the second as null.
#[repr(C)]
pub struct CallbackBinding {
    /// The Context the script runs under; captured at bind time.
    pub ctx: *mut Context,
    /// The language function value's code pointer (a wrapper taking
    /// `(ctx, env, args...)`, host C calling convention).
    pub code: *const u8,
    /// The language function value's environment pointer (null for a
    /// non-capturing function — the only kind usable as a C callback, C5).
    pub env: *const u8,
    /// The first userdata the script registered, passed back to the
    /// language callback unchanged.
    pub userdata1: *mut u8,
    /// The second userdata (§14.4); null when the callback-info carries
    /// only one slot.
    pub userdata2: *mut u8,
}

/// The script execution context.
///
/// `repr(C)` with a fixed prefix that generated code reads directly
/// (the runtime's ABI contract):
///
/// | offset | field | read by |
/// |---|---|---|
/// | 0 | trap flag (`u32`) | every emitted trap check |
/// | 4 | reload epoch (`u32`) | coroutine resume, hot-reload mode |
/// | 8 | function table (`*const *const u8`) | script calls, hot-reload mode |
/// | 16 | module-global block (`*mut u8`) | global access, hot-reload mode |
///
/// Everything past the prefix is opaque to generated code. The three
/// hot-reload fields are read only by code lowered in reload mode; the
/// AOT tier and the plain dev-JIT run never touch them.
#[repr(C)]
pub struct Context {
    trap_flag: u32,
    reload_epoch: u32,
    fn_table: *const *const u8,
    globals: *mut u8,
    script_depth: u32,
    allocations: HashMap<usize, Allocation>,
    stdout: Vec<u8>,
    trap: Option<TrapRecord>,
    trap_observer: Option<TrapObserver>,
    trap_observer_userdata: *mut c_void,
    trap_observer_active: bool,
    interned: HashMap<(usize, usize), usize>,
    shadow: Vec<(usize, usize)>,
    roots: Vec<(usize, usize)>,
    callbacks: Vec<Box<CallbackBinding>>,
    // Transient P13 JSON output builders. Untracked serializers create
    // no active-reference set; tracked ones do so explicitly.
    json_builders: crate::json::JsonBuilders,
    // Transient P13 parsed syntax trees. They contain no language
    // allocations and are removed before JSON.parse returns.
    json_parsers: crate::json::JsonParsers,
    // Ship-tier policy flag (§8.1a): when true, `delete`/`collect` free
    // and forget immediately; when false (dev tier), they retain and
    // poison so use-after-delete/double-delete trap.
    release_on_delete: bool,
    // The `Math.random` PRNG (stdlib.md §2), default-seeded on every
    // construction path so dev and ship draw the same contract stream.
    rng: crate::math::Rng,
    // The `Date.now` source (stdlib.md §3): `Some` pins the clock
    // (tests, replays); `None` reads the system UTC clock.
    now_override: Option<i64>,
    // One-shot object-request allocation fault. `Some(n)` refuses the
    // n-th subsequent Context::alloc request; underlying arena chunk
    // allocations are deliberately not counted because their sequence
    // is tier-specific.
    alloc_fail_countdown: Option<u64>,
    // ----- ship-tier arena state (§8.1b); empty on the dev tier -----
    // Every chunk, in creation order.
    chunks: Vec<Chunk>,
    // (chunk base address, index into `chunks`), sorted by base: the
    // membership lookup (binary search for the covering chunk).
    chunk_map: Vec<(usize, usize)>,
    // Per-class LIFO free list head: a freed payload address, 0 when
    // empty; the next link is the freed payload's first word.
    free_heads: [usize; NUM_CLASSES],
    // Per-class chunk currently being bump-allocated (index into
    // `chunks`), if any.
    open: [Option<usize>; NUM_CLASSES],
    // Allocations above LARGEST_BLOCK, keyed by payload address.
    large: HashMap<usize, LargeAlloc>,
    #[cfg(test)]
    stats: std::sync::Arc<ArenaStats>,
}

impl Context {
    /// Creates an empty context.
    ///
    /// Development-tier policy: `unsafeDelete`/`collect` retain and poison
    /// (double delete and use-after-delete trap). The dev-JIT builds its
    /// Context this way. For the AOT/ship tier use
    /// [`Context::new_releasing`].
    #[must_use]
    pub fn new() -> Box<Context> {
        Self::with_policy(false)
    }

    /// Creates an empty ship-tier context (§8.1a/§8.1b). Unlike
    /// [`Context::new`]'s retain-and-poison dev policy, `unsafeDelete` and
    /// `collect` here **release immediately**: a size-classed block goes
    /// back to its arena free list and a large allocation is freed
    /// outright — no per-allocation table is kept. Use-after-delete and
    /// double delete are undefined (Q6/§8.1b), not trapped. The AOT host
    /// entry ([`crate::ffi::sub_rt_ctx_new`]) builds its Context this way.
    #[must_use]
    pub fn new_releasing() -> Box<Context> {
        Self::with_policy(true)
    }

    fn with_policy(release_on_delete: bool) -> Box<Context> {
        Box::new(Context {
            trap_flag: 0,
            reload_epoch: 0,
            fn_table: std::ptr::null(),
            globals: std::ptr::null_mut(),
            script_depth: 0,
            allocations: HashMap::new(),
            stdout: Vec::new(),
            trap: None,
            trap_observer: None,
            trap_observer_userdata: std::ptr::null_mut(),
            trap_observer_active: false,
            interned: HashMap::new(),
            shadow: Vec::new(),
            roots: Vec::new(),
            callbacks: Vec::new(),
            json_builders: crate::json::JsonBuilders::default(),
            json_parsers: crate::json::JsonParsers::default(),
            release_on_delete,
            rng: crate::math::Rng::new(crate::math::DEFAULT_RANDOM_SEED),
            now_override: None,
            alloc_fail_countdown: None,
            chunks: Vec::new(),
            chunk_map: Vec::new(),
            free_heads: [0; NUM_CLASSES],
            open: [None; NUM_CLASSES],
            large: HashMap::new(),
            #[cfg(test)]
            stats: Default::default(),
        })
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
    }

    /// Number of host-to-script calls currently on the stack. A hot
    /// reload is applied only at zero (the frame-boundary rule).
    #[must_use]
    pub fn script_depth(&self) -> u32 {
        self.script_depth
    }

    // ----- Math.random state (stdlib.md §2) -----

    /// Draws the next `Math.random()` value from the Context-owned
    /// xoshiro256++ stream.
    pub fn random_f64(&mut self) -> f64 {
        self.rng.next_f64()
    }

    /// Reseeds the `Math.random` stream by re-expanding `seed` (host
    /// replay control; [`crate::ffi::sub_rt_ctx_seed_random`]).
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
    /// [`crate::ffi::sub_rt_ctx_set_now`]). Every later `Date.now()`
    /// returns exactly `ms` until pinned again.
    pub fn set_now(&mut self, ms: i64) {
        self.now_override = Some(ms);
    }

    // ----- trap state -----

    /// Records a trap. The first trap wins; later ones are ignored
    /// (generated code unwinds after the first, but runtime functions
    /// invoked on the unwind path stay callable).
    pub fn trap(&mut self, kind: TrapKind, message: impl Into<String>, pos_id: u32) {
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
    pub fn set_trap_observer(
        &mut self,
        observer: Option<TrapObserver>,
        userdata: *mut c_void,
    ) {
        self.trap_observer = observer;
        self.trap_observer_userdata = if observer.is_some() {
            userdata
        } else {
            std::ptr::null_mut()
        };
    }

    /// Runs `call` while the observer-active guard is raised.
    ///
    /// The drop guard also restores the flag during a Rust unwind. The
    /// public observer type is `extern "C"`, whose callbacks may not
    /// unwind across the ABI boundary; this cleanup additionally covers
    /// every unwind path Rust can construct before such a boundary.
    fn with_trap_observer_active(&mut self, call: impl FnOnce()) {
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
    /// Only trap *reporting* state and unfinished transient JSON builders
    /// are touched. Allocations, globals, roots, the stdout sink, and the
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
        self.trap = None;
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

    /// Appends `bytes` and a trailing newline to the stdout sink
    /// (print never writes to the process stdout).
    pub fn print_line(&mut self, bytes: &[u8]) {
        self.stdout.extend_from_slice(bytes);
        self.stdout.push(b'\n');
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

    /// Allocates `size` payload bytes tagged `class_id`; returns the
    /// zeroed payload pointer, or null after recording an
    /// allocation-failure trap. Dev tier: an individual system
    /// allocation tracked in the map. Ship tier: served from the arena
    /// (§8.1b).
    pub fn alloc(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
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
        if self.release_on_delete {
            return self.arena_alloc(size, class_id, pos_id);
        }
        let total = HEADER_SIZE.saturating_add(size.max(1));
        let Ok(layout) = Layout::from_size_align(total, 16) else {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes is not representable"),
                pos_id,
            );
            return std::ptr::null_mut();
        };
        // SAFETY: `layout` has non-zero size (>= HEADER_SIZE + 1).
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes failed"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: `base` is a fresh allocation of at least HEADER_SIZE
        // bytes; the header writes stay inside it.
        unsafe {
            (base as *mut u64).write(LIVE_STATE);
            (base.add(8) as *mut u32).write(class_id);
            (base.add(12) as *mut u32).write(pos_id);
        }
        // SAFETY: HEADER_SIZE <= total.
        let payload = unsafe { base.add(HEADER_SIZE) };
        self.allocations.insert(
            payload as usize,
            Allocation {
                base,
                layout,
                payload_size: size,
                live: true,
                marked: false,
            },
        );
        payload
    }

    // ----- ship-tier arena internals (§8.1b) -----

    /// Size-class index for a total block size `need`
    /// (`need <= LARGEST_BLOCK`): the smallest power-of-two block that
    /// holds it, `SMALLEST_BLOCK` at minimum.
    #[inline]
    fn size_class(need: usize) -> usize {
        let rounded = need.next_power_of_two().max(SMALLEST_BLOCK);
        (rounded.trailing_zeros() - SMALLEST_BLOCK.trailing_zeros()) as usize
    }

    /// Ship-tier `alloc`: free-list pop, else bump from the class's open
    /// chunk, else a new chunk; above the largest class, an individual
    /// system allocation with a `LargeAlloc` record. The payload is
    /// zeroed in every case (§8.1b: conservative tracing and language
    /// zero-init rely on it).
    fn arena_alloc(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
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
            // the payload's first word; after unlinking, the whole
            // capacity is re-zeroed and the header re-armed.
            unsafe {
                self.free_heads[class] = (payload as *const usize).read();
                std::ptr::write_bytes(payload, 0, block_size - HEADER_SIZE);
                let base = payload.sub(HEADER_SIZE);
                (base as *mut u64).write(LIVE_STATE);
                (base.add(8) as *mut u32).write(class_id);
                (base.add(12) as *mut u32).write(pos_id);
            }
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
            (base as *mut u64).write(LIVE_STATE);
            (base.add(8) as *mut u32).write(class_id);
            (base.add(12) as *mut u32).write(pos_id);
            base.add(HEADER_SIZE)
        };
        chunk.bump += 1;
        payload
    }

    /// Allocates and registers a fresh chunk for `class`; returns its
    /// index in `chunks`, or `None` after an allocation-failure trap.
    fn arena_new_chunk(&mut self, class: usize, pos_id: u32) -> Option<usize> {
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
        let pos = self
            .chunk_map
            .partition_point(|&(b, _)| b < base as usize);
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
    fn arena_alloc_large(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
        let total = HEADER_SIZE.saturating_add(size.max(1));
        let Ok(layout) = Layout::from_size_align(total, 16) else {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes is not representable"),
                pos_id,
            );
            return std::ptr::null_mut();
        };
        // SAFETY: `layout` has non-zero size (>= HEADER_SIZE + 1).
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes failed"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: `base` is a fresh allocation of at least HEADER_SIZE
        // bytes; the header writes stay inside it.
        let payload = unsafe {
            (base as *mut u64).write(LIVE_STATE);
            (base.add(8) as *mut u32).write(class_id);
            (base.add(12) as *mut u32).write(pos_id);
            base.add(HEADER_SIZE)
        };
        self.large.insert(
            payload as usize,
            LargeAlloc {
                base,
                layout,
                payload_size: size,
            },
        );
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
    fn arena_lookup_block(&self, addr: usize) -> Option<(*mut u8, usize)> {
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
        Some((unsafe { chunk.base.add(bi * chunk.block_size) }, chunk.class))
    }

    /// Clears the separately allocated storage owned by a Map/Set before
    /// its header is retired.
    ///
    /// `payload` must be a live Map/Set header owned by this Context.
    fn clear_container_on_delete(&mut self, payload: usize) {
        #[cfg(test)]
        self.stats
            .container_delete_entries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // SAFETY: callers reach this helper only after exact membership,
        // liveness, and Map/Set class-id checks.
        unsafe { crate::assocops::clear(self, payload as *mut u8) };
    }

    /// Ship-tier release: a live classed block goes to its class's free
    /// list; a large record is freed and dropped. Map/Set storage is
    /// cleared after the same membership/header read that release already
    /// requires, avoiding a container-specific lookup before every
    /// ordinary delete. Anything else — dead, free-listed, or unknown —
    /// is undefined per Q6/§8.1b and handled as a no-op (never a trap,
    /// never relied upon).
    fn arena_release(&mut self, payload: usize) {
        if let Some((block, class)) = self.arena_lookup_block(payload) {
            // SAFETY: `block` heads a block inside an owned chunk; both
            // header reads stay inside it.
            let class_id = unsafe {
                if (block as *const u64).read() != LIVE_STATE {
                    return;
                }
                (block.add(8) as *const u32).read()
            };
            if matches!(class_id, CLASS_MAP | CLASS_SET) {
                self.clear_container_on_delete(payload);
            }
            // SAFETY: the live-grid check above proves `block` is still
            // owned. Container clearing only retires its separate child
            // allocations, so the state word and payload free-list link
            // remain valid.
            unsafe {
                (block as *mut u64).write(DEAD_STATE);
                (payload as *mut usize).write(self.free_heads[class]);
                self.free_heads[class] = payload;
            }
            return;
        }
        if let Some(a) = self.large.remove(&payload) {
            // SAFETY: removing the exact live record proves `a.base`
            // heads an owned allocation whose class-id word is readable.
            let class_id = unsafe { (a.base.add(8) as *const u32).read() };
            if matches!(class_id, CLASS_MAP | CLASS_SET) {
                self.clear_container_on_delete(payload);
            }
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

    /// Frees or marks the allocation at `payload` dead, per tier policy.
    ///
    /// Development tier ([`Context::new`]): the bytes are retained
    /// (poisoned) so stale handles trap; double delete and unknown
    /// pointers trap (Q6).
    ///
    /// Ship tier ([`Context::new_releasing`], §8.1b): a classed block is
    /// pushed onto its arena free list; a large allocation is freed and
    /// its record dropped. A double delete or unknown pointer is
    /// undefined (Q6/§8.1b) and handled as a no-op (no trap).
    pub fn delete(&mut self, payload: usize, pos_id: u32) {
        if self.release_on_delete {
            self.arena_release(payload);
            return;
        }
        match self.allocations.get_mut(&payload) {
            None => {
                self.trap(
                    TrapKind::InvalidDelete,
                    "unsafeDelete of a pointer the Context does not own",
                    pos_id,
                );
                return;
            }
            Some(a) if !a.live => {
                self.trap(
                    TrapKind::DoubleDelete,
                    "unsafeDelete of an already-deleted allocation",
                    pos_id,
                );
                return;
            }
            Some(a) => {
                // SAFETY: exact map membership and `a.live` prove the
                // allocation owns a readable header.
                let class_id = unsafe { (a.base.add(8) as *const u32).read() };
                if matches!(class_id, CLASS_MAP | CLASS_SET) {
                    // End the allocation-table borrow before clearing:
                    // backing storage is retired through recursive
                    // `delete` calls on this Context.
                } else {
                    a.live = false;
                    // SAFETY: `base` is owned by this context and at
                    // least HEADER_SIZE bytes; poisoning the state word
                    // makes the emitted use-after-delete checks fire.
                    unsafe { (a.base as *mut u64).write(DEAD_STATE) };
                    return;
                }
            }
        }
        self.clear_container_on_delete(payload);
        if let Some(a) = self.allocations.get_mut(&payload) {
            a.live = false;
            // SAFETY: `base` is owned by this context and at least
            // HEADER_SIZE bytes; poisoning the state word makes the
            // emitted use-after-delete checks fire.
            unsafe { (a.base as *mut u64).write(DEAD_STATE) };
        } else {
            self.trap(
                TrapKind::Internal,
                "Map/Set header disappeared while deleting its storage",
                pos_id,
            );
        }
    }

    /// True when `payload` is a live allocation (test/inspection aid).
    /// Ship tier: the exact membership test — chunk range, block grid,
    /// bump watermark, live header — or a large record (§8.1b).
    #[must_use]
    pub fn is_live(&self, payload: usize) -> bool {
        if self.release_on_delete {
            if let Some((block, _)) = self.arena_lookup_block(payload) {
                // SAFETY: `block` heads a block inside an owned chunk.
                return unsafe { (block as *const u64).read() } == LIVE_STATE;
            }
            return self.large.contains_key(&payload);
        }
        self.allocations.get(&payload).is_some_and(|a| a.live)
    }

    /// Validates a runtime-operation receiver under Q6's tier policy.
    ///
    /// Development retains exact allocation membership and traps stale
    /// handles. Ship-tier use-after-delete is undefined, so the runtime
    /// preserves its existing unchecked behavior there.
    pub(crate) fn require_live_handle(&mut self, payload: usize, pos_id: u32) -> bool {
        if self.trapped() {
            return false;
        }
        if self.release_on_delete || self.is_live(payload) {
            return true;
        }
        self.trap(
            TrapKind::UseAfterDelete,
            "use of a deleted allocation",
            pos_id,
        );
        false
    }

    /// Number of live allocations (test/inspection aid). Ship tier: a
    /// chunk walk (live blocks below each watermark) plus the large
    /// records.
    #[must_use]
    pub fn live_count(&self) -> usize {
        if self.release_on_delete {
            let mut n = self.large.len();
            for chunk in &self.chunks {
                for bi in 0..chunk.bump {
                    // SAFETY: `bi < bump`, so the block is inside the
                    // owned chunk; only its state word is read.
                    let state =
                        unsafe { (chunk.base.add(bi * chunk.block_size) as *const u64).read() };
                    if state == LIVE_STATE {
                        n += 1;
                    }
                }
            }
            return n;
        }
        self.allocations.values().filter(|a| a.live).count()
    }

    /// Payload capacity in live allocations.
    ///
    /// The development tier reports exact requested payload sizes. The
    /// ship tier reports size-class payload capacity for arena blocks and
    /// exact payload size for large allocations.
    #[must_use]
    pub fn live_bytes(&self) -> usize {
        if self.release_on_delete {
            let mut bytes = self
                .large
                .values()
                .fold(0usize, |sum, a| sum.saturating_add(a.payload_size));
            for chunk in &self.chunks {
                for bi in 0..chunk.bump {
                    // SAFETY: `bi < bump`, so the state word lies inside
                    // the Context-owned chunk.
                    let state =
                        unsafe { (chunk.base.add(bi * chunk.block_size) as *const u64).read() };
                    if state == LIVE_STATE {
                        bytes = bytes.saturating_add(chunk.block_size - HEADER_SIZE);
                    }
                }
            }
            return bytes;
        }
        self.allocations
            .values()
            .filter(|a| a.live)
            .fold(0usize, |sum, a| sum.saturating_add(a.payload_size))
    }

    /// Bytes currently reserved from the system for Context allocations.
    ///
    /// Development-tier deleted allocations remain reserved until the
    /// Context is dropped. Ship-tier chunks remain reserved, while a
    /// deleted large allocation is returned to the system immediately.
    #[must_use]
    pub fn reserved_bytes(&self) -> usize {
        if self.release_on_delete {
            let chunk_bytes = self
                .chunks
                .iter()
                .fold(0usize, |sum, c| sum.saturating_add(c.layout.size()));
            return self.large.values().fold(chunk_bytes, |sum, a| {
                sum.saturating_add(a.layout.size())
            });
        }
        self.allocations
            .values()
            .fold(0usize, |sum, a| sum.saturating_add(a.layout.size()))
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
        if self.release_on_delete {
            for chunk in &self.chunks {
                for bi in 0..chunk.bump {
                    // SAFETY: `bi < bump`, so the complete header lies
                    // inside this Context-owned chunk.
                    let base = unsafe { chunk.base.add(bi * chunk.block_size) };
                    // SAFETY: the state word lies in the header.
                    if unsafe { (base as *const u64).read() } != LIVE_STATE {
                        continue;
                    }
                    // SAFETY: class_id and pos_id are initialized header
                    // words for every live block.
                    let (class_id, pos_id) = unsafe {
                        (
                            (base.add(8) as *const u32).read(),
                            (base.add(12) as *const u32).read(),
                        )
                    };
                    // SAFETY: the host supplied `visitor` and its userdata.
                    unsafe {
                        visitor(
                            userdata,
                            class_id,
                            pos_id,
                            (chunk.block_size - HEADER_SIZE) as u64,
                        )
                    };
                    count += 1;
                }
            }
            for allocation in self.large.values() {
                // Every retained large record is live. The complete
                // header was initialized by `arena_alloc_large`.
                let (class_id, pos_id) = unsafe {
                    (
                        (allocation.base.add(8) as *const u32).read(),
                        (allocation.base.add(12) as *const u32).read(),
                    )
                };
                // SAFETY: the host supplied `visitor` and its userdata.
                unsafe {
                    visitor(
                        userdata,
                        class_id,
                        pos_id,
                        allocation.payload_size as u64,
                    )
                };
                count += 1;
            }
            return count;
        }

        for allocation in self.allocations.values().filter(|a| a.live) {
            // SAFETY: a live retained allocation owns a fully initialized
            // header for the lifetime of the Context.
            let (class_id, pos_id) = unsafe {
                (
                    (allocation.base.add(8) as *const u32).read(),
                    (allocation.base.add(12) as *const u32).read(),
                )
            };
            // SAFETY: the host supplied `visitor` and its userdata.
            unsafe {
                visitor(
                    userdata,
                    class_id,
                    pos_id,
                    allocation.payload_size as u64,
                )
            };
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
    pub fn root_add(&mut self, base: usize, words: usize) {
        self.roots.push((base, words));
    }

    /// Pushes a shadow frame: `slots` consecutive 8-byte slots at
    /// `base`, each holding a managed local (or null).
    pub fn shadow_push(&mut self, base: usize, slots: usize) {
        self.shadow.push((base, slots));
    }

    /// Pops the most recent shadow frame.
    pub fn shadow_pop(&mut self) {
        self.shadow.pop();
    }

    /// Explicitly invoked collection (Q7): frees every allocation not
    /// reachable from the registered roots. Never runs unbidden.
    pub fn collect(&mut self) {
        let mut work: Vec<usize> = Vec::new();
        for &(base, words) in &self.roots {
            for i in 0..words {
                // SAFETY: root ranges are addresses of live global
                // slots registered by generated code; reading their
                // words is valid for the duration of the script run.
                work.push(unsafe { ((base + i * 8) as *const usize).read_unaligned() });
            }
        }
        for &(base, slots) in &self.shadow {
            for i in 0..slots {
                // SAFETY: shadow frames are live stack ranges registered
                // by the running generated code.
                work.push(unsafe { ((base + i * 8) as *const usize).read_unaligned() });
            }
        }
        work.extend(self.interned.values().copied());

        if self.release_on_delete {
            // Ship tier (§8.1b): mark state lives in the block header
            // (MARK_STATE), not in a map; sweep walks the chunk grids and
            // the large records.
            self.arena_mark(&mut work);
            self.arena_sweep();
            return;
        }

        while let Some(addr) = work.pop() {
            let Some(a) = self.allocations.get_mut(&addr) else {
                continue;
            };
            if !a.live || a.marked {
                continue;
            }
            a.marked = true;
            let payload = addr as *const u8;
            let words = a.payload_size / 8;
            for i in 0..words {
                // SAFETY: the payload is owned by this context and at
                // least `payload_size` bytes; reads stay inside it.
                let w = unsafe { (payload.add(i * 8) as *const usize).read_unaligned() };
                work.push(w);
            }
        }

        for a in self.allocations.values_mut() {
            if a.live && !a.marked {
                a.live = false;
                // SAFETY: as in `delete`: poison the retained header.
                unsafe { (a.base as *mut u64).write(DEAD_STATE) };
            }
            a.marked = false;
        }
    }

    /// Ship-tier mark phase (§8.1b): drains the conservative work list.
    /// A word is treated as a managed payload only under the exact
    /// membership test ([`Context::arena_lookup_block`] plus a live
    /// header, or an exact large-record match); a reached block's header
    /// is stamped `MARK_STATE` and its payload words are pushed.
    fn arena_mark(&mut self, work: &mut Vec<usize>) {
        while let Some(addr) = work.pop() {
            let (block, payload_size) = if let Some((block, class)) =
                self.arena_lookup_block(addr)
            {
                // SAFETY: `block` heads a block inside an owned chunk;
                // the state read stays inside it.
                let state = unsafe { (block as *const u64).read() };
                if state != LIVE_STATE {
                    // Dead, free-listed, or already marked.
                    continue;
                }
                // Allocation zeroes the complete size-class payload
                // capacity, so conservatively tracing its padding is safe.
                // The final header word now carries allocation attribution.
                (block, (SMALLEST_BLOCK << class) - HEADER_SIZE)
            } else if let Some(a) = self.large.get(&addr) {
                // SAFETY: `base` heads an owned large allocation.
                let state = unsafe { (a.base as *const u64).read() };
                if state != LIVE_STATE {
                    // Already marked.
                    continue;
                }
                (a.base, a.payload_size)
            } else {
                continue;
            };
            // SAFETY: `block` heads an owned allocation whose payload is
            // at least `payload_size` bytes; all accesses stay inside it.
            unsafe {
                (block as *mut u64).write(MARK_STATE);
                let payload = block.add(HEADER_SIZE);
                for i in 0..payload_size / 8 {
                    work.push((payload.add(i * 8) as *const usize).read_unaligned());
                }
            }
        }
    }

    /// Ship-tier sweep (§8.1b): walk every chunk's grid up to its bump
    /// watermark — unreached live blocks join their class free list,
    /// marked survivors are restored to `LIVE_STATE` — then free every
    /// unreached large record and restore the marked ones.
    fn arena_sweep(&mut self) {
        for ci in 0..self.chunks.len() {
            let (cbase, block_size, class, bump) = {
                let c = &self.chunks[ci];
                (c.base, c.block_size, c.class, c.bump)
            };
            for bi in 0..bump {
                // SAFETY: `bi < bump`, so the block is inside the owned
                // chunk; the state word and the payload's first word (the
                // free-list link) stay inside it.
                unsafe {
                    let block = cbase.add(bi * block_size);
                    match (block as *const u64).read() {
                        MARK_STATE => (block as *mut u64).write(LIVE_STATE),
                        LIVE_STATE => {
                            (block as *mut u64).write(DEAD_STATE);
                            let payload = block.add(HEADER_SIZE);
                            (payload as *mut usize).write(self.free_heads[class]);
                            self.free_heads[class] = payload as usize;
                        }
                        // DEAD_STATE: already on the free list.
                        _ => {}
                    }
                }
            }
        }
        // Cannot `remove`+`dealloc` while iterating, so collect the
        // unreached payload addresses first.
        let mut freed: Vec<usize> = Vec::new();
        for (&addr, a) in &self.large {
            // SAFETY: `base` heads an owned large allocation.
            unsafe {
                match (a.base as *const u64).read() {
                    MARK_STATE => (a.base as *mut u64).write(LIVE_STATE),
                    _ => freed.push(addr),
                }
            }
        }
        for addr in freed {
            if let Some(a) = self.large.remove(&addr) {
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

    /// Allocates an immutable string; payload = `[len: u64][bytes]`.
    /// Returns the payload pointer (the string handle).
    pub fn alloc_str(&mut self, bytes: &[u8], pos_id: u32) -> *mut u8 {
        let p = self.alloc(8 + bytes.len(), CLASS_STRING, pos_id);
        if p.is_null() {
            return p;
        }
        // SAFETY: `p` points at a fresh allocation of 8 + len bytes.
        unsafe {
            (p as *mut u64).write(bytes.len() as u64);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(8), bytes.len());
        }
        p
    }

    /// Reads the bytes of a string handle. The borrow is tied to
    /// `&self`: string storage lives as long as the context and is
    /// immutable, and `&self` prevents freeing while the slice is
    /// alive.
    ///
    /// # Safety
    ///
    /// `handle` must be a string payload produced by [`Context::alloc_str`]
    /// on this context and still owned by it.
    #[must_use]
    pub unsafe fn str_bytes(&self, handle: *const u8) -> &[u8] {
        // SAFETY: caller guarantees `handle` is a live string payload;
        // its first 8 bytes are the length of the following bytes.
        unsafe {
            let len = (handle as *const u64).read() as usize;
            std::slice::from_raw_parts(handle.add(8), len)
        }
    }

    /// The address of a string handle's UTF-8 bytes (the C `const char*`
    /// half of a `(ptr, len)` string view; the length is
    /// [`Context::str_bytes`]`.len()`, also `sub_rt_str_len`). Skips the
    /// 8-byte length prefix of the string payload.
    ///
    /// # Safety
    ///
    /// `handle` must be a live string payload of this context.
    #[must_use]
    pub unsafe fn str_data(&self, handle: *const u8) -> *const u8 {
        // SAFETY: the bytes follow the 8-byte length prefix.
        unsafe { handle.add(8) }
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

    /// Registers a C-callback binding and returns a stable pointer to it
    /// (P5.2b). The pointer is what a boundary marshaler stores in a C
    /// `void* userdata` slot; the generic trampoline
    /// ([`crate::ffi::sub_rt_cb_trampoline`]) reads the binding back
    /// through it. Bindings live for the whole Context (the Q13 lifetime
    /// rule), so the pointer stays valid for every later callback.
    ///
    /// Both userdata slots (§14.4) are stored and delivered to the language
    /// callback; a one-slot callback-info passes `userdata2` as null.
    pub fn bind_callback(
        &mut self,
        code: *const u8,
        env: *const u8,
        userdata1: *mut u8,
        userdata2: *mut u8,
    ) -> *mut u8 {
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

    /// Allocates an empty dynamic array with `elem_size`-byte elements.
    pub fn array_new(&mut self, elem_size: usize, pos_id: u32) -> *mut u8 {
        let p = self.alloc(std::mem::size_of::<ArrayHeader>(), CLASS_ARRAY, pos_id);
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
            });
        }
        p
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
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        if h.len == h.cap {
            let new_cap = if h.cap == 0 { 4 } else { h.cap * 2 };
            let elem = h.elem_size as usize;
            let new_data = self.alloc(new_cap as usize * elem, CLASS_ARRAY_DATA, pos_id);
            if new_data.is_null() {
                return -1;
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
                if self.release_on_delete {
                    // Ship tier (§8.1b): retired data blocks flow through
                    // the same free-list/large-record release path as
                    // `delete`, so array growth does not accumulate.
                    self.arena_release(old);
                } else {
                    if let Some(a) = self.allocations.get_mut(&old) {
                        a.live = false;
                        // SAFETY: poisons the retained header, as in
                        // `delete`.
                        unsafe { (a.base as *mut u64).write(DEAD_STATE) };
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
        h.len as i32
    }

    /// Removes the last element, copying it to `dst`. Traps on empty.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context; `dst`
    /// must be writable for the array's element size.
    pub unsafe fn array_pop(&mut self, handle: *mut u8, dst: *mut u8, pos_id: u32) {
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        if h.len == 0 {
            self.trap(TrapKind::EmptyPop, "pop() on an empty array", pos_id);
            return;
        }
        h.len -= 1;
        let elem = h.elem_size as usize;
        // SAFETY: the removed slot holds an initialized element; `dst`
        // is writable per the caller contract.
        unsafe {
            std::ptr::copy_nonoverlapping(h.data.add(h.len as usize * elem), dst, elem);
        }
    }

    /// Returns the address of element `idx`, or null after an
    /// out-of-bounds trap.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context.
    pub unsafe fn array_elem_ptr(&mut self, handle: *mut u8, idx: i32, pos_id: u32) -> *mut u8 {
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &*(handle as *const ArrayHeader) };
        if idx < 0 || idx as u64 >= h.len {
            let len = h.len;
            self.trap(
                TrapKind::IndexOutOfBounds,
                format!("index {idx} out of bounds for array length {len}"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: 0 <= idx < len <= cap.
        unsafe { h.data.add(idx as usize * h.elem_size as usize) }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        for a in self.allocations.values() {
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `Context::alloc` and are freed exactly once, here.
            unsafe { dealloc(a.base, a.layout) };
        }
        // Ship-tier arena (§8.1b): chunks and large records are freed
        // wholesale — Context-scoped memory.
        for c in &self.chunks {
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `arena_new_chunk` and are freed exactly once, here.
            unsafe { dealloc(c.base, c.layout) };
        }
        for a in self.large.values() {
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `arena_alloc_large` and are freed exactly once, here.
            unsafe { dealloc(a.base, a.layout) };
        }
        #[cfg(test)]
        {
            self.stats
                .chunks
                .fetch_sub(self.chunks.len(), std::sync::atomic::Ordering::SeqCst);
            self.stats
                .large
                .fetch_sub(self.large.len(), std::sync::atomic::Ordering::SeqCst);
        }
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("trap_flag", &self.trap_flag)
            .field("allocations", &self.allocations.len())
            .field("stdout_len", &self.stdout.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Context {
        /// Enumerable allocation count. Dev tier: total map length (live
        /// + retained-dead), distinguishing retain-and-poison (entry
        /// kept) from release (entry gone). Ship tier (§8.1b): there is
        /// no per-allocation table; the enumerable set is the live
        /// blocks plus large records, i.e. `live_count` — a released
        /// block leaves nothing behind.
        fn allocation_count(&self) -> usize {
            if self.release_on_delete {
                self.live_count()
            } else {
                self.allocations.len()
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
    }

    // The emitted-C SsArrayHeader (codegen/src/cemit.rs, §10a) mirrors this
    // layout; a reorder here is caught by this test.
    #[test]
    fn array_header_offsets_match_the_abi_contract() {
        assert_eq!(core::mem::offset_of!(ArrayHeader, len), 0);
        assert_eq!(core::mem::offset_of!(ArrayHeader, cap), 8);
        assert_eq!(core::mem::offset_of!(ArrayHeader, elem_size), 16);
        assert_eq!(core::mem::offset_of!(ArrayHeader, data), 24);
        assert_eq!(core::mem::size_of::<ArrayHeader>(), 32);
    }

    #[test]
    fn random_stream_is_default_seeded_on_both_construction_paths() {
        // Dev (`new`) and ship (`new_releasing`) Contexts draw the same
        // contract stream (stdlib.md §2).
        let mut dev = Context::new();
        let mut ship = Context::new_releasing();
        let reference: Vec<u64> = {
            let mut r = crate::math::Rng::new(crate::math::DEFAULT_RANDOM_SEED);
            (0..8).map(|_| r.next_f64().to_bits()).collect()
        };
        let dev_draws: Vec<u64> = (0..8).map(|_| dev.random_f64().to_bits()).collect();
        let ship_draws: Vec<u64> = (0..8).map(|_| ship.random_f64().to_bits()).collect();
        assert_eq!(dev_draws, reference);
        assert_eq!(ship_draws, reference);
    }

    #[test]
    fn seed_random_restarts_the_stream() {
        let mut ctx = Context::new();
        ctx.seed_random(7);
        let first: Vec<u64> = (0..4).map(|_| ctx.random_f64().to_bits()).collect();
        ctx.seed_random(7);
        let again: Vec<u64> = (0..4).map(|_| ctx.random_f64().to_bits()).collect();
        assert_eq!(first, again);
    }

    #[test]
    fn now_defaults_to_the_system_clock_and_pins_on_set() {
        let mut ctx = Context::new();
        // Unpinned: a valid, non-decreasing time value from the system
        // clock (stdlib.md §3).
        let a = ctx.now_utc_ms();
        let b = ctx.now_utc_ms();
        assert!(crate::date::in_range(a), "system clock out of TimeClip: {a}");
        assert!(b >= a);
        // Pinned: exactly the set value, stable across reads, negative
        // (pre-1970) values included.
        ctx.set_now(123);
        assert_eq!(ctx.now_utc_ms(), 123);
        assert_eq!(ctx.now_utc_ms(), 123);
        ctx.set_now(-456);
        assert_eq!(ctx.now_utc_ms(), -456);
    }

    #[test]
    fn trap_flag_is_at_offset_zero() {
        let ctx = Context::new();
        let base = &*ctx as *const Context as usize;
        let flag = &ctx.trap_flag as *const u32 as usize;
        assert_eq!(flag - base, Context::trap_flag_offset());
    }

    #[test]
    fn reload_prefix_offsets_match_the_abi_contract() {
        let ctx = Context::new();
        let base = &*ctx as *const Context as usize;
        assert_eq!(
            &ctx.reload_epoch as *const u32 as usize - base,
            Context::reload_epoch_offset()
        );
        assert_eq!(
            &ctx.fn_table as *const *const *const u8 as usize - base,
            Context::fn_table_offset()
        );
        assert_eq!(
            &ctx.globals as *const *mut u8 as usize - base,
            Context::globals_offset()
        );
    }

    #[test]
    fn reload_epoch_and_script_depth_track_swaps_and_entries() {
        let mut ctx = Context::new();
        assert_eq!(ctx.reload_epoch(), 0);
        ctx.bump_reload_epoch();
        assert_eq!(ctx.reload_epoch(), 1);
        assert_eq!(ctx.script_depth(), 0);
        ctx.enter_script();
        ctx.enter_script();
        assert_eq!(ctx.script_depth(), 2);
        ctx.exit_script();
        ctx.exit_script();
        ctx.exit_script();
        assert_eq!(ctx.script_depth(), 0);
    }

    #[test]
    fn observer_active_guard_blocks_clear_and_resets_on_rust_unwind() {
        let mut ctx = Context::new();
        ctx.trap(TrapKind::EmptyPop, "pending", 1);
        ctx.trap_observer_active = true;
        assert!(!ctx.can_clear_trap());
        let p: *mut Context = &mut *ctx;
        // SAFETY: this is a host-boundary probe over a live Context. The
        // test raises only the guard bit, without entering a callback and
        // therefore without creating an aliasing violation.
        assert_eq!(unsafe { crate::ffi::sub_rt_ctx_clear_trap(p) }, 0);
        assert!(ctx.trapped(), "the refused clear changed trap state");
        ctx.trap_observer_active = false;
        assert!(ctx.can_clear_trap());

        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.with_trap_observer_active(|| panic!("test observer unwind"));
        }));
        assert!(unwind.is_err());
        assert!(
            !ctx.trap_observer_active,
            "the observer-active flag remained stuck after a Rust unwind"
        );
        assert!(ctx.can_clear_trap());
    }

    #[test]
    fn fn_table_and_globals_pointers_round_trip() {
        let mut ctx = Context::new();
        let table: [*const u8; 2] = [std::ptr::null(), std::ptr::null()];
        let mut block = [0u8; 16];
        ctx.set_fn_table(table.as_ptr());
        ctx.set_globals(block.as_mut_ptr());
        assert_eq!(ctx.fn_table, table.as_ptr());
        assert_eq!(ctx.globals, block.as_mut_ptr());
    }

    #[test]
    fn print_appends_bytes_and_newline_to_the_sink() {
        let mut ctx = Context::new();
        ctx.print_line(b"hello");
        ctx.print_line(b"x");
        assert_eq!(ctx.take_stdout(), b"hello\nx\n");
        assert!(ctx.take_stdout().is_empty());
    }

    #[test]
    fn alloc_is_zeroed_tagged_and_live() {
        let mut ctx = Context::new();
        let p = ctx.alloc(24, 3, 41);
        assert!(!p.is_null());
        assert!(ctx.is_live(p as usize));
        // SAFETY: p is a fresh 24-byte payload with a 16-byte header.
        unsafe {
            assert_eq!((p.offset(STATE_OFFSET as isize) as *const u64).read(), LIVE_STATE);
            assert_eq!((p.offset(CLASS_ID_OFFSET as isize) as *const u32).read(), 3);
            assert_eq!((p.offset(POS_ID_OFFSET as isize) as *const u32).read(), 41);
            for i in 0..24 {
                assert_eq!(p.add(i).read(), 0);
            }
        }
    }

    #[test]
    fn allocation_fault_counts_object_requests_identically_in_both_tiers() {
        for (tier, mut ctx) in [
            ("dev", Context::new()),
            ("ship", Context::new_releasing()),
        ] {
            ctx.fail_alloc_after(2);
            assert!(!ctx.alloc(8, 1, 10).is_null(), "{tier}: first request");
            assert!(ctx.alloc(5000, 2, 11).is_null(), "{tier}: second request");
            let trap = ctx.trap_record().expect("injected allocation trap");
            assert_eq!(trap.kind, TrapKind::AllocationFailure, "{tier}");
            assert_eq!(trap.pos_id, 11, "{tier}");
            assert_eq!(trap.message, "injected allocation failure", "{tier}");

            ctx.clear_trap();
            assert!(
                !ctx.alloc(8, 3, 12).is_null(),
                "{tier}: fired fault is one-shot"
            );
            ctx.fail_alloc_after(0);
            assert!(
                !ctx.alloc(8, 4, 13).is_null(),
                "{tier}: zero disables injection"
            );
        }
    }

    #[test]
    fn live_allocation_visitor_reports_class_position_and_tier_bytes() {
        unsafe extern "C" fn collect(
            userdata: *mut c_void,
            class_id: u32,
            pos_id: u32,
            payload_bytes: u64,
        ) {
            // SAFETY: the test passes a live Vec of this exact type.
            let triples =
                unsafe { &mut *userdata.cast::<Vec<(u32, u32, u64)>>() };
            triples.push((class_id, pos_id, payload_bytes));
        }

        for (tier, mut ctx, expected) in [
            (
                "dev",
                Context::new(),
                vec![(10u32, 20u32, 1u64), (12u32, 22u32, 5000u64)],
            ),
            (
                "ship",
                Context::new_releasing(),
                vec![(10u32, 20u32, 16u64), (12u32, 22u32, 5000u64)],
            ),
        ] {
            let first = ctx.alloc(1, 10, 20);
            let deleted = ctx.alloc(40, 11, 21);
            let large = ctx.alloc(5000, 12, 22);
            assert!(!first.is_null() && !deleted.is_null() && !large.is_null());
            ctx.delete(deleted as usize, 23);

            let mut triples = Vec::new();
            // SAFETY: `collect` receives a live Vec as userdata.
            let visited = unsafe {
                ctx.visit_live_allocations(
                    Some(collect),
                    (&mut triples as *mut Vec<(u32, u32, u64)>).cast(),
                )
            };
            triples.sort_unstable();
            assert_eq!(visited, 2, "{tier}");
            assert_eq!(triples, expected, "{tier}");
            assert_eq!(visited as usize, ctx.live_count(), "{tier}");
        }
    }

    #[test]
    fn memory_accounting_walks_live_blocks_without_running_counters() {
        let mut measured = Vec::new();
        for (tier, mut ctx) in [
            ("dev", Context::new()),
            ("ship", Context::new_releasing()),
        ] {
            let first = ctx.alloc(1, 1, 0);
            let deleted = ctx.alloc(17, 1, 0);
            let large = ctx.alloc(5000, 1, 0);
            assert!(!first.is_null() && !deleted.is_null() && !large.is_null());
            assert_eq!(ctx.live_count(), 3, "{tier}: N allocations");
            let reserved_before = ctx.reserved_bytes();

            ctx.delete(deleted as usize, 0);
            assert_eq!(ctx.live_count(), 2, "{tier}: N-M allocations");
            assert_eq!(
                ctx.reserved_bytes(),
                reserved_before,
                "{tier}: deleting a size-class allocation must retain its storage"
            );
            measured.push((
                tier,
                ctx.live_count(),
                ctx.live_bytes(),
                ctx.reserved_bytes(),
            ));
        }

        assert_eq!(measured[0], ("dev", 2, 5001, 5066));
        assert_eq!(measured[1], ("ship", 2, 5016, 136_088));
        assert_eq!(
            measured[0].1, measured[1].1,
            "live allocation count is tier-independent"
        );
        assert_ne!(
            measured[0].2, measured[1].2,
            "size-class payload capacity makes live bytes tier-dependent"
        );
        assert_ne!(
            measured[0].3, measured[1].3,
            "arena chunks make reserved bytes tier-dependent"
        );
    }

    #[test]
    fn delete_poisons_and_double_delete_traps() {
        let mut ctx = Context::new();
        let p = ctx.alloc(8, 1, 0);
        ctx.delete(p as usize, 5);
        assert!(!ctx.is_live(p as usize));
        // SAFETY: bytes are retained after delete (dev-tier policy).
        unsafe {
            assert_eq!((p.offset(STATE_OFFSET as isize) as *const u64).read(), DEAD_STATE);
        }
        assert!(!ctx.trapped());
        ctx.delete(p as usize, 6);
        assert!(ctx.trapped());
        let r = ctx.trap_record().expect("trap recorded");
        assert_eq!(r.kind, TrapKind::DoubleDelete);
        assert_eq!(r.pos_id, 6);
    }

    #[test]
    fn ordinary_delete_skips_container_path_and_ship_has_one_membership_lookup() {
        use std::sync::atomic::Ordering::SeqCst;

        for mut ctx in [Context::new(), Context::new_releasing()] {
            let releasing = ctx.release_on_delete;
            let ordinary = ctx.alloc(16, 1, 0);
            let stats = ctx.test_stats();
            let lookups_before = stats.membership_lookups.load(SeqCst);
            let container_entries_before = stats.container_delete_entries.load(SeqCst);

            ctx.delete(ordinary as usize, 0);

            assert_eq!(
                stats.container_delete_entries.load(SeqCst),
                container_entries_before,
                "an ordinary allocation must not enter the Map/Set delete path"
            );
            assert_eq!(
                stats.membership_lookups.load(SeqCst) - lookups_before,
                usize::from(releasing),
                "ship delete must combine class resolution with its one release lookup"
            );

            // Prove the path counter is live, not a vacuous zero.
            let map =
                crate::assocops::new(&mut ctx, 4, 4, crate::assocops::KeyKind::Bits, false, 0);
            ctx.delete(map as usize, 0);
            assert_eq!(
                stats.container_delete_entries.load(SeqCst),
                container_entries_before + 1
            );
        }
    }

    #[test]
    fn ship_mode_delete_frees_and_removes_the_entry() {
        let mut ctx = Context::new_releasing();
        let a = ctx.alloc(8, 1, 0);
        let b = ctx.alloc(8, 1, 0);
        assert_eq!(ctx.live_count(), 2);
        assert_eq!(ctx.allocation_count(), 2);

        ctx.delete(a as usize, 0);
        assert_eq!(ctx.live_count(), 1);
        // The block is released, not merely marked dead.
        assert!(!ctx.is_live(a as usize));
        assert_eq!(ctx.allocation_count(), 1, "ship mode leaves no entry behind");

        // A second delete of the now-released pointer does NOT trap
        // (undefined-but-safe no-op, §8.1b), unlike the dev-mode
        // double-delete trap covered by
        // `delete_poisons_and_double_delete_traps`. (Checked before the
        // next alloc: the arena's LIFO free list would hand `a`'s block
        // back, making a later delete of `a` a live delete.)
        ctx.delete(a as usize, 9);
        assert!(!ctx.trapped());

        // A fresh allocation still succeeds after the release.
        let c = ctx.alloc(8, 1, 0);
        assert!(!c.is_null());
        assert!(ctx.is_live(c as usize));

        // Contrast: an equivalent dev-mode delete retains the entry.
        let mut dev = Context::new();
        let d0 = dev.alloc(8, 1, 0);
        let _d1 = dev.alloc(8, 1, 0);
        assert_eq!(dev.allocation_count(), 2);
        dev.delete(d0 as usize, 0);
        assert_eq!(
            dev.allocation_count(),
            2,
            "dev mode retains the poisoned entry"
        );
        // `b` and `c` keep the ship context's live set non-trivial.
        assert!(ctx.is_live(b as usize));
    }

    #[test]
    fn delete_of_unowned_pointer_traps() {
        let mut ctx = Context::new();
        ctx.delete(0x1000, 1);
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::InvalidDelete));
    }

    #[test]
    fn clear_trap_resets_reporting_state_and_nothing_else() {
        let mut ctx = Context::new();
        let kept = ctx.alloc(8, 1, 0);
        let deleted = ctx.alloc(8, 1, 0);
        ctx.delete(deleted as usize, 0);
        ctx.print_line(b"before");
        ctx.bump_reload_epoch();

        ctx.trap(TrapKind::EmptyPop, "pop() on an empty array", 3);
        assert!(ctx.trapped());
        ctx.clear_trap();

        // Reporting state is gone...
        assert!(!ctx.trapped());
        assert!(ctx.trap_record().is_none());
        assert_eq!(ctx.trap_flag, 0, "the offset-0 flag is the cleared bit");
        // ...and nothing else moved.
        assert!(ctx.is_live(kept as usize));
        assert!(!ctx.is_live(deleted as usize), "a deleted allocation stays dead");
        assert_eq!(ctx.reload_epoch(), 1, "staleness survives the clear");
        assert_eq!(ctx.stdout_bytes(), b"before\n");

        // A later fault records normally (the first-trap-wins rule is
        // per uncleared run, not per Context).
        ctx.trap(TrapKind::DivisionByZero, "integer division by zero", 9);
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::DivisionByZero, 9))
        );
    }

    #[test]
    fn clear_trap_on_an_untrapped_context_is_a_no_op() {
        let mut ctx = Context::new();
        ctx.clear_trap();
        assert!(!ctx.trapped());
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn first_trap_wins() {
        let mut ctx = Context::new();
        ctx.trap(TrapKind::EmptyPop, "first", 1);
        ctx.trap(TrapKind::DivisionByZero, "second", 2);
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::EmptyPop);
        assert_eq!(r.pos_id, 1);
    }

    #[test]
    fn collect_frees_unreachable_and_keeps_rooted() {
        let mut ctx = Context::new();
        let kept = ctx.alloc(8, 1, 0);
        let dropped = ctx.alloc(8, 1, 0);
        let mut slot: usize = kept as usize;
        let slot_ptr: *mut usize = &mut slot;
        ctx.root_add(slot_ptr as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(kept as usize));
        assert!(!ctx.is_live(dropped as usize));
        // Dropping the last reference frees the rest on the next
        // collect. Written through the registered pointer — the same
        // way generated code updates its shadow slots.
        // SAFETY: `slot` is alive for the whole test.
        unsafe { slot_ptr.write(0) };
        ctx.collect();
        assert!(!ctx.is_live(kept as usize));
    }

    #[test]
    fn ship_mode_collect_frees_and_removes_unreachable() {
        let mut ctx = Context::new_releasing();
        let kept = ctx.alloc(8, 1, 0);
        let _dropped = ctx.alloc(8, 1, 0);
        assert_eq!(ctx.allocation_count(), 2);
        let mut slot: usize = kept as usize;
        ctx.root_add(&mut slot as *mut usize as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(kept as usize));
        // The unreachable block is released, not poisoned.
        assert_eq!(ctx.allocation_count(), 1, "ship mode releases swept blocks");
        assert_eq!(ctx.live_count(), 1);
    }

    #[test]
    fn collect_traces_through_payload_words() {
        let mut ctx = Context::new();
        let inner = ctx.alloc(8, 1, 0);
        let outer = ctx.alloc(8, 1, 0);
        // outer.field0 = inner
        // SAFETY: outer payload is 8 writable bytes.
        unsafe { (outer as *mut usize).write(inner as usize) };
        let mut slot: usize = outer as usize;
        ctx.root_add(&mut slot as *mut usize as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(outer as usize));
        assert!(ctx.is_live(inner as usize));
    }

    #[test]
    fn root_ranges_scan_every_word() {
        // A two-word root range (e.g. a global FixedArray of two
        // references): both interior handles must survive collection.
        let mut ctx = Context::new();
        let a = ctx.alloc(8, 1, 0);
        let b = ctx.alloc(8, 1, 0);
        let range = [a as usize, b as usize];
        ctx.root_add(range.as_ptr() as usize, 2);
        ctx.collect();
        assert!(ctx.is_live(a as usize));
        assert!(ctx.is_live(b as usize));
    }

    #[test]
    fn shadow_frames_root_locals_and_pop_unroots_them() {
        let mut ctx = Context::new();
        let p = ctx.alloc(8, 1, 0);
        let slots = [p as usize];
        ctx.shadow_push(slots.as_ptr() as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(p as usize));
        ctx.shadow_pop();
        ctx.collect();
        assert!(!ctx.is_live(p as usize));
    }

    #[test]
    fn strings_alloc_read_and_intern() {
        let mut ctx = Context::new();
        let h = ctx.alloc_str(b"alpha-beta", 0);
        // SAFETY: h is a live string handle from this context.
        unsafe {
            assert_eq!(ctx.str_bytes(h), b"alpha-beta");
        }
        static LIT: &[u8] = b"hello";
        // SAFETY: LIT is 'static.
        let a = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
        // SAFETY: as above.
        let b = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
        assert_eq!(a, b, "literal interning reuses one allocation");
        // Interned literals survive collection with no other roots.
        ctx.collect();
        assert!(ctx.is_live(a as usize));
    }

    #[test]
    fn arrays_push_index_pop_and_traps() {
        let mut ctx = Context::new();
        let h = ctx.array_new(4, 0);
        // SAFETY: h is a live array handle; sources/dests are valid.
        unsafe {
            assert_eq!(ctx.array_len(h), 0);
            for v in [10i32, 20, 30, 40, 50] {
                let n = ctx.array_push(h, &v as *const i32 as *const u8, 1);
                assert!(n > 0);
            }
            assert_eq!(ctx.array_len(h), 5);
            let p2 = ctx.array_elem_ptr(h, 2, 2);
            assert_eq!((p2 as *const i32).read(), 30);
            let mut out: i32 = 0;
            ctx.array_pop(h, &mut out as *mut i32 as *mut u8, 3);
            assert_eq!(out, 50);
            assert_eq!(ctx.array_len(h), 4);
            // OOB traps and returns null.
            assert!(ctx.array_elem_ptr(h, 4, 9).is_null());
        }
        let r = ctx.trap_record().expect("oob trap");
        assert_eq!(r.kind, TrapKind::IndexOutOfBounds);
        assert_eq!(r.pos_id, 9);
    }

    #[test]
    fn array_elem_size_reports_the_creation_size() {
        let mut ctx = Context::new();
        let a = ctx.array_new(4, 0);
        let b = ctx.array_new(16, 0);
        // SAFETY: live array handles of this context.
        unsafe {
            assert_eq!(ctx.array_elem_size(a), 4);
            assert_eq!(ctx.array_elem_size(b), 16);
        }
    }

    #[test]
    fn empty_pop_traps() {
        let mut ctx = Context::new();
        let h = ctx.array_new(4, 0);
        let mut out: i32 = 0;
        // SAFETY: h is a live array handle; dst is valid.
        unsafe { ctx.array_pop(h, &mut out as *mut i32 as *mut u8, 7) };
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::EmptyPop));
    }

    #[test]
    fn array_data_is_reached_by_conservative_marking() {
        let mut ctx = Context::new();
        let h = ctx.array_new(8, 0);
        let inner = ctx.alloc(8, 1, 0);
        // SAFETY: valid array handle and element source.
        unsafe {
            let v = inner as usize;
            ctx.array_push(h, &v as *const usize as *const u8, 0);
        }
        let mut slot: usize = h as usize;
        ctx.root_add(&mut slot as *mut usize as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(h as usize));
        assert!(ctx.is_live(inner as usize), "element reached via data pointer");
    }

    // §8.1a: array growth in the ship tier frees each retired data block
    // instead of retaining it poisoned, so allocation_count does not grow
    // with the number of capacity doublings. The dev tier retains them.
    #[test]
    fn ship_mode_array_growth_frees_retired_blocks() {
        // Push enough u32 elements to force several capacity doublings
        // (cap: 0 -> 4 -> 8 -> 16 -> 32), retiring the old data block each
        // time (4 retires for 20 pushes).
        let pushes = 20u32;

        let mut ship = Context::new_releasing();
        let sh = ship.array_new(4, 0);
        for v in 0..pushes {
            // SAFETY: sh is a live u32 array handle; src is a valid u32.
            let n = unsafe { ship.array_push(sh, &v as *const u32 as *const u8, 0) };
            assert!(n > 0);
        }
        // Header + current live data block only; retired blocks are freed
        // and removed, so the map holds a small constant, not 1 + N.
        let ship_count = ship.allocation_count();
        assert!(
            ship_count <= 2,
            "ship tier should hold header + live data only, got {ship_count}"
        );

        let mut dev = Context::new();
        let dh = dev.array_new(4, 0);
        for v in 0..pushes {
            // SAFETY: dh is a live u32 array handle; src is a valid u32.
            let n = unsafe { dev.array_push(dh, &v as *const u32 as *const u8, 0) };
            assert!(n > 0);
        }
        // Dev tier retains every retired block poisoned, so the map is
        // strictly larger.
        let dev_count = dev.allocation_count();
        assert!(
            dev_count > ship_count,
            "dev tier retains retired blocks: dev {dev_count} vs ship {ship_count}"
        );
    }

    // ----- ship-tier arena (§8.1b) -----

    // alloc→delete→alloc of the same class pops the free-listed block
    // (LIFO: the same address comes back) and never grows a chunk.
    #[test]
    fn ship_arena_reuses_free_listed_block_without_chunk_growth() {
        let mut ctx = Context::new_releasing();
        let first = ctx.alloc(16, 1, 0);
        assert!(!first.is_null());
        assert_eq!(ctx.chunk_count(), 1);
        for _ in 0..10_000 {
            ctx.delete(first as usize, 0);
            let again = ctx.alloc(16, 1, 0);
            assert_eq!(again, first, "LIFO free list returns the same block");
        }
        assert_eq!(ctx.chunk_count(), 1, "reuse cycles must not grow chunks");
        assert_eq!(ctx.live_count(), 1);
    }

    // Free-list reuse must return a zeroed payload (§8.1b): the free-list
    // link occupies the payload's first word and the previous contents
    // the rest, so both must be scrubbed.
    #[test]
    fn ship_arena_free_list_reuse_returns_zeroed_payload() {
        let mut ctx = Context::new_releasing();
        let p = ctx.alloc(16, 1, 0);
        // SAFETY: p is a live 16-byte payload.
        unsafe { std::ptr::write_bytes(p, 0xAB, 16) };
        ctx.delete(p as usize, 0);
        let q = ctx.alloc(16, 1, 0);
        assert_eq!(q, p, "the dirtied block is the one reused");
        // SAFETY: q is a live 16-byte payload.
        unsafe {
            for i in 0..16 {
                assert_eq!(q.add(i).read(), 0, "byte {i} not zeroed on reuse");
            }
        }
    }

    // Context drop frees every chunk and large record (no leak), observed
    // through the test-only resource balance that outlives the Context.
    #[test]
    fn ship_context_drop_frees_all_chunks_and_large_records() {
        let mut ctx = Context::new_releasing();
        // Several classes, enough small blocks for a real chunk, and two
        // large records (one deleted before the drop).
        for _ in 0..100 {
            assert!(!ctx.alloc(16, 1, 0).is_null());
            assert!(!ctx.alloc(200, 1, 0).is_null());
        }
        let big = ctx.alloc(LARGEST_BLOCK + 1, 2, 0);
        let big2 = ctx.alloc(64 * 1024, 2, 0);
        assert!(!big.is_null() && !big2.is_null());
        ctx.delete(big2 as usize, 0);
        let stats = ctx.test_stats();
        use std::sync::atomic::Ordering::SeqCst;
        assert!(stats.chunks.load(SeqCst) >= 2, "distinct classes use distinct chunks");
        assert_eq!(stats.large.load(SeqCst), 1);
        drop(ctx);
        assert_eq!(stats.chunks.load(SeqCst), 0, "drop must free every chunk");
        assert_eq!(stats.large.load(SeqCst), 0, "drop must free every large record");
    }

    // Arena edition of the collect tests: unreachable classed blocks are
    // released (live_count drops, storage is reusable), rooted ones
    // survive with their header restored to LIVE_STATE.
    #[test]
    fn ship_collect_releases_unreachable_and_keeps_rooted() {
        let mut ctx = Context::new_releasing();
        let kept = ctx.alloc(16, 1, 0);
        let dropped = ctx.alloc(16, 1, 0);
        let dropped2 = ctx.alloc(16, 1, 0);
        // kept.field0 = inner: reached transitively through the
        // header-recorded payload size.
        let inner = ctx.alloc(16, 1, 0);
        // SAFETY: kept payload is 16 writable bytes.
        unsafe { (kept as *mut usize).write(inner as usize) };
        let mut slot: usize = kept as usize;
        let slot_ptr: *mut usize = &mut slot;
        ctx.root_add(slot_ptr as usize, 1);
        assert_eq!(ctx.live_count(), 4);
        ctx.collect();
        assert!(ctx.is_live(kept as usize));
        assert!(ctx.is_live(inner as usize), "traced through payload words");
        assert!(!ctx.is_live(dropped as usize));
        assert!(!ctx.is_live(dropped2 as usize));
        assert_eq!(ctx.live_count(), 2);
        // Survivor headers are LIVE again (mark state fully restored).
        // SAFETY: kept is a live payload with a 16-byte header.
        unsafe {
            assert_eq!(
                (kept.offset(STATE_OFFSET as isize) as *const u64).read(),
                LIVE_STATE
            );
        }
        // Swept blocks are on the free list: the next same-class alloc
        // reuses one instead of bumping.
        let chunks = ctx.chunk_count();
        let reused = ctx.alloc(16, 1, 0);
        assert!(reused == dropped || reused == dropped2);
        assert_eq!(ctx.chunk_count(), chunks);
        // Unrooting frees the rest on the next collect.
        // SAFETY: `slot` is alive for the whole test.
        unsafe { slot_ptr.write(0) };
        ctx.collect();
        assert!(!ctx.is_live(kept as usize));
        assert!(!ctx.is_live(inner as usize));
    }

    // The large-record path (§8.1b): membership is an exact address
    // match, tracing uses the record's payload size, collect frees an
    // unreached record, and delete frees immediately.
    #[test]
    fn ship_large_allocations_membership_trace_collect_and_delete() {
        let mut ctx = Context::new_releasing();
        let big = ctx.alloc(2 * LARGEST_BLOCK, 1, 0);
        assert!(!big.is_null());
        assert!(ctx.is_live(big as usize));
        assert!(!ctx.is_live(big as usize + 8), "interior address is not a payload");
        assert_eq!(ctx.live_count(), 1);
        // A classed block referenced from the large payload's interior
        // survives collect: the record's size drives the trace.
        let inner = ctx.alloc(16, 1, 0);
        // SAFETY: big is a live payload of 2*LARGEST_BLOCK bytes.
        unsafe { (big.add(LARGEST_BLOCK) as *mut usize).write(inner as usize) };
        let mut slot: usize = big as usize;
        let slot_ptr: *mut usize = &mut slot;
        ctx.root_add(slot_ptr as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(big as usize));
        assert!(ctx.is_live(inner as usize), "traced through the large payload");
        // Unrooted, collect frees the record (and the inner block).
        // SAFETY: `slot` is alive for the whole test.
        unsafe { slot_ptr.write(0) };
        ctx.collect();
        assert!(!ctx.is_live(big as usize));
        assert_eq!(ctx.live_count(), 0);
        use std::sync::atomic::Ordering::SeqCst;
        assert_eq!(ctx.test_stats().large.load(SeqCst), 0);
        // Direct delete of a large allocation frees it too.
        let big3 = ctx.alloc(LARGEST_BLOCK + 100, 1, 0);
        assert_eq!(ctx.test_stats().large.load(SeqCst), 1);
        let reserved_before_delete = ctx.reserved_bytes();
        ctx.delete(big3 as usize, 0);
        assert!(!ctx.is_live(big3 as usize));
        assert_eq!(ctx.test_stats().large.load(SeqCst), 0);
        assert!(
            ctx.reserved_bytes() < reserved_before_delete,
            "a large delete returns its individual allocation to the system"
        );
    }

    // The exact membership test (§8.1b): chunk range, block grid, bump
    // watermark, live header — all four. Near-miss addresses are not
    // blocks: is_live says no and delete is a no-op that never traps and
    // never corrupts the free list.
    #[test]
    fn ship_membership_rejects_off_grid_and_above_watermark_addresses() {
        let mut ctx = Context::new_releasing();
        let p = ctx.alloc(16, 1, 0);
        let q = ctx.alloc(16, 1, 0);
        assert_eq!(ctx.live_count(), 2);
        // Off-grid: interior of a live payload.
        assert!(!ctx.is_live(p as usize + 8));
        ctx.delete(p as usize + 8, 0);
        // In-chunk but above the bump watermark (the next grid slot).
        let next_slot = q as usize + SMALLEST_BLOCK;
        assert!(!ctx.is_live(next_slot));
        ctx.delete(next_slot, 0);
        // Outside any chunk.
        assert!(!ctx.is_live(0x1000));
        ctx.delete(0x1000, 0);
        assert!(!ctx.trapped(), "ship-tier delete never traps");
        assert_eq!(ctx.live_count(), 2, "no-ops must not release live blocks");
        assert!(ctx.is_live(p as usize) && ctx.is_live(q as usize));
        // The allocator still works: both blocks delete and reuse cleanly.
        ctx.delete(p as usize, 0);
        ctx.delete(q as usize, 0);
        let r = ctx.alloc(16, 1, 0);
        assert_eq!(r, q, "free list intact after the no-op deletes");
    }

    // Ship-tier interning and array retirement ride the arena: interned
    // strings survive collect (roots), and retired array data blocks land
    // on free lists without growing the live set.
    #[test]
    fn ship_interned_strings_and_arrays_work_on_the_arena() {
        let mut ctx = Context::new_releasing();
        static LIT: &[u8] = b"arena-lit";
        // SAFETY: LIT is 'static.
        let a = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
        // SAFETY: as above.
        let b = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
        assert_eq!(a, b);
        ctx.collect();
        assert!(ctx.is_live(a as usize), "interned literal is a root");
        // SAFETY: a is a live string handle of this context.
        unsafe { assert_eq!(ctx.str_bytes(a), b"arena-lit") };

        let h = ctx.array_new(4, 0);
        for v in 0..20u32 {
            // SAFETY: h is a live u32 array handle; src is a valid u32.
            let n = unsafe { ctx.array_push(h, &v as *const u32 as *const u8, 0) };
            assert!(n > 0);
        }
        // SAFETY: h is a live array handle.
        unsafe {
            assert_eq!(ctx.array_len(h), 20);
            let p7 = ctx.array_elem_ptr(h, 7, 0);
            assert_eq!((p7 as *const u32).read(), 7);
        }
    }
}
