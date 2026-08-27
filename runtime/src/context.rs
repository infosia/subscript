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
//! Freed-handle diagnostics (§8.1a-3) are disabled by default. When a host
//! enables them before the first allocation, `Context.free` and
//! `Context.collect()` can mark an allocation dead and poison its header.
//! Retention is limited by a requested-payload threshold and a layout-byte
//! budget; oldest records are evicted first when the budget fills.
//!
//! Ship-tier policy (§8.1b): no per-allocation map. Blocks up to the
//! largest size class are carved from Context-owned per-class chunks by
//! bump pointer; `Context.free` pushes a block onto its class's LIFO
//! free list (threaded through the freed payload's first word) and the
//! next same-class `alloc` pops it, zeroed. Larger allocations are
//! individual system allocations with their own record. Double delete
//! and use-after-delete are undefined here (Q6, trusted scripts). Enabling
//! freed-handle diagnostics switches either construction path to exact-size
//! allocation and budgeted retain-and-poison at or above the configured
//! payload threshold.
//!
//! # Collection
//!
//! `Context.collect()` never runs unbidden (design invariant 2). Roots are the
//! addresses generated code registers: module-global slots
//! (`root_add`) and per-call shadow frames of managed locals
//! (`shadow_push`/`shadow_pop`). Marking is conservative: the payload
//! of every reached allocation is scanned for pointer-aligned words
//! that equal a live payload address (this covers reference-class
//! fields, array elements, array data pointers, and coroutine frame
//! slots without layout metadata). Conservative marking can retain
//! garbage; it never frees a reachable allocation.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::hash::{BuildHasherDefault, Hasher};

use crate::trap::{TrapKind, TrapRecord};
use crate::worker::{PostResult, Worker, WorkerEntry, WorkerInit, WorkerOutcome, WorkerSet};

/// Host callback invoked when a Context records its first trap.
///
/// The callback deliberately receives no [`Context`] handle. It runs
/// inside [`Context::trap`] while that method holds exclusive access to
/// the Context, so calling any `subscript_rt_*` API that takes that Context
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

/// Host callback invoked for each line printed while it is installed.
///
/// The callback receives no [`Context`] handle. It runs inside
/// [`Context::print_line`] while that method holds exclusive access to the
/// Context, so calling any `subscript_rt_*` API that takes that Context
/// (including through a pointer smuggled in `userdata`) would violate
/// Rust's aliasing rules and is undefined behaviour.
///
/// `line` excludes the trailing newline and is valid only for the duration
/// of the callback.
pub type PrintObserver =
    unsafe extern "C" fn(userdata: *mut c_void, line: *const u8, line_len: u64);

/// Host callback invoked for optional runtime diagnostics advisories.
///
/// The callback deliberately receives no [`Context`] handle. It runs while
/// the Context is exclusively borrowed, so calling any `subscript_rt_*` API
/// that takes that Context (including through a pointer smuggled in
/// `userdata`) would violate Rust's aliasing rules and is undefined
/// behaviour. The callback is observation-only and must not call back into
/// script.
///
/// `message` is valid only for the duration of the callback.
pub type DiagnosticsObserver = unsafe extern "C" fn(
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
pub type AllocationVisitor =
    unsafe extern "C" fn(userdata: *mut c_void, class_id: u32, pos_id: u32, payload_bytes: u64);

/// C calling convention shared by the module initializer (`subscript_init`) and every
/// supported host export.
///
/// A host that may clear traps brackets each call with
/// `subscript_rt_ctx_enter_script` and `subscript_rt_ctx_exit_script`.
///
/// An ordinary run entry uses `subscript_export_main`; a host-owned entry may instead
/// drive other zero-argument `void` exports using the symbol
/// `subscript_export_<name>` and this same C signature.
pub type ScriptMainEntry = unsafe extern "C" fn(ctx: *mut Context);

/// Fixed ABI of a compiler-generated async-frame resume function.
///
/// `frame` is a Context-owned coroutine frame and `out` points at storage
/// for the fulfilled value (null for the zero-argument `Promise<void>` root
/// exports driven by the runtime). The return is 1 on completion and 0 on
/// suspension.
pub type AsyncResume = unsafe extern "C" fn(ctx: *mut Context, frame: *mut u8, out: *mut u8) -> u8;

#[derive(Clone, Copy)]
struct AsyncRoot {
    frame: *mut u8,
    resume: AsyncResume,
}

#[derive(Default)]
struct AsyncFrameMeta {
    created_epoch: u32,
    completion: Option<Vec<u8>>,
}

/// Bytes between an allocation's base and its payload.
pub const HEADER_SIZE: usize = 16;
/// Recommended byte ceiling for freed-handle diagnostic retention.
///
/// The runtime treats this as an ordinary literal budget; hosts may pass any
/// other value accepted by [`Context::set_freed_handle_diagnostics`].
pub const FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES: usize = 1_073_741_824;
/// Advisory kind reported when `Context.free` releases registered callback
/// userdata.
pub const DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE: u32 = 1;
/// Advisory kind reported when the callback-binding count reaches its
/// host-configured threshold.
pub const DIAGNOSTICS_ADVISORY_BINDING_COUNT: u32 = 2;
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

// A BMP string produced by `for…of` contains exactly one Unicode scalar.
// Encode that scalar in an odd handle (Context allocations are
// 16-byte-aligned) and serve its bytes from immutable process data.
// Astral scalars use ordinary allocated string handles, interned for the
// Context's lifetime; keeping the tagged range BMP-only makes the two
// handle forms disjoint.
const BMP_CODE_POINT_COUNT: usize = 0x1_0000;
const INLINE_STRING_TAG: usize = 1;

const fn encode_utf8_word(scalar: u32) -> u32 {
    let mut bytes = [0u8; 4];
    if scalar <= 0x7f {
        bytes[0] = scalar as u8;
    } else if scalar <= 0x7ff {
        bytes[0] = 0xc0 | (scalar >> 6) as u8;
        bytes[1] = 0x80 | (scalar & 0x3f) as u8;
    } else if scalar <= 0xffff {
        bytes[0] = 0xe0 | (scalar >> 12) as u8;
        bytes[1] = 0x80 | ((scalar >> 6) & 0x3f) as u8;
        bytes[2] = 0x80 | (scalar & 0x3f) as u8;
    } else {
        bytes[0] = 0xf0 | (scalar >> 18) as u8;
        bytes[1] = 0x80 | ((scalar >> 12) & 0x3f) as u8;
        bytes[2] = 0x80 | ((scalar >> 6) & 0x3f) as u8;
        bytes[3] = 0x80 | (scalar & 0x3f) as u8;
    }
    u32::from_ne_bytes(bytes)
}

const fn code_point_utf8_table() -> [u32; BMP_CODE_POINT_COUNT] {
    let mut table = [0u32; BMP_CODE_POINT_COUNT];
    let mut scalar = 0;
    while scalar < BMP_CODE_POINT_COUNT {
        table[scalar] = encode_utf8_word(scalar as u32);
        scalar += 1;
    }
    table
}

static CODE_POINT_UTF8: [u32; BMP_CODE_POINT_COUNT] = code_point_utf8_table();

fn inline_string_scalar(handle: *const u8) -> Option<u32> {
    let raw = handle as usize;
    if raw & INLINE_STRING_TAG == 0 {
        return None;
    }
    let encoded = raw >> 1;
    (encoded > 0 && encoded <= BMP_CODE_POINT_COUNT).then_some((encoded - 1) as u32)
}

fn scalar_utf8_len(scalar: u32) -> usize {
    match scalar {
        0..=0x7f => 1,
        0x80..=0x7ff => 2,
        0x800..=0xffff => 3,
        _ => 4,
    }
}

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
/// Class id used for RegExp handles.
pub const CLASS_REGEX: u32 = 0xFFFF_FF09;

// ----- ship-tier arena (§8.1b) -----

// Header state word for a block reached by the current `Context.collect()` mark
// phase (ship tier only). Lives only between mark and sweep — no script
// code runs during `Context.collect`, and sweep restores survivors to
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
pub(crate) struct ArenaStats {
    chunks: std::sync::atomic::AtomicUsize,
    large: std::sync::atomic::AtomicUsize,
    membership_lookups: std::sync::atomic::AtomicUsize,
    container_delete_entries: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl ArenaStats {
    pub(crate) fn owned_resources(&self) -> (usize, usize) {
        use std::sync::atomic::Ordering;

        (
            self.chunks.load(Ordering::SeqCst),
            self.large.load(Ordering::SeqCst),
        )
    }
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
    marked: bool,
}

/// Host allocations used only while recursively lowering one foreign-call
/// argument tree (§32/§33). They are deliberately outside the managed heap:
/// a callback may collect while C is borrowing scratch arrays or pointed-to
/// structs. Nested foreign calls take a length mark and release only their
/// own suffix.
struct BoundaryScratchAllocation {
    base: *mut u8,
    layout: Layout,
}

/// Ship-tier module-global storage owned directly by one Context.
struct ModuleGlobals {
    base: *mut u8,
    layout: Layout,
}

/// One retained-and-poisoned exact-size allocation, in retirement order.
struct RetainedAllocation {
    payload: usize,
    base: *mut u8,
    layout: Layout,
}

/// Fast deterministic hashing for Context-owned, 16-byte-aligned payload
/// addresses. Script data cannot choose these keys, so randomized hashing
/// buys no collision-resistance here.
#[derive(Default)]
struct AddressHasher(u64);

impl Hasher for AddressHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // HashSet<usize> dispatches to `write_usize`; keep a complete
        // implementation for the Hasher contract and future refactors.
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        self.0 = hash;
    }

    fn write_usize(&mut self, value: usize) {
        // Remove the guaranteed alignment zeros, then use the FxHash
        // multiplier to spread adjacent allocator addresses.
        self.0 = ((value >> 4) as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type AddressSet = HashSet<usize, BuildHasherDefault<AddressHasher>>;

// Valid boundary callbacks have a null `env`, so §14.4a exposes the
// `(code, userdata1, userdata2)` identity. Keeping `env` in the internal
// key prevents invalid non-null environments from aliasing in release
// builds, where the premise assertion is disabled.
type CallbackIdentity = (*const u8, *const u8, *mut u8, *mut u8);

/// A registered C-callback binding (P5.2b). The language's function value
/// is a `(code, env)` pair with the calling convention `(ctx, env,
/// args...)`; a C callback wants a bare `(fnptr, void* userdata)`. A
/// generic C-ABI trampoline ([`crate::ffi::subscript_rt_cb_trampoline`]) bridges
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
/// | 16 | module-global block (`*mut u8`) | global access, ship C and hot reload |
///
/// Everything past the prefix is opaque to generated code. The function
/// table and reload epoch are read only by code lowered in reload mode;
/// ship C and hot-reload code both read the module-global block slot.
#[repr(C)]
pub struct Context {
    trap_flag: u32,
    reload_epoch: u32,
    fn_table: *const *const u8,
    globals: *mut u8,
    // The ship C tier installs one layout-fixed block here. Reload mode
    // instead points `globals` at storage owned by its ReloadSession and
    // leaves this empty.
    module_globals: Option<ModuleGlobals>,
    // Parent-owned worker handles. Queue synchronization lives entirely in
    // the worker module; the Context itself remains thread-affine.
    workers: WorkerSet,
    script_depth: u32,
    // Q34 pending root invocations, in host kick order. Active frames are
    // tracked separately while a callback is on the stack so explicit
    // collection keeps the whole current frame chain alive.
    async_roots: VecDeque<AsyncRoot>,
    active_async_frames: Vec<usize>,
    // §70 held async handles. The reference count itself occupies the
    // frame header's four-byte `reserved` word; Context metadata holds only
    // reload provenance and the fulfilled bytes needed by a later holder.
    async_frames: HashMap<usize, AsyncFrameMeta>,
    // Exact-size live allocations. The dev tier uses this path; a ship
    // Context switches to it when freed-handle diagnostics are enabled.
    // Collection marks and sweeps only this map.
    allocations: HashMap<usize, Allocation>,
    boundary_scratch: Vec<BoundaryScratchAllocation>,
    // Diagnostic-mode retained-and-poisoned addresses. Exact membership
    // preserves double-delete classification without putting dead records
    // in the map collection sweeps.
    dead_allocations: AddressSet,
    // FIFO ownership records for retained-dead backing allocations. The
    // collector never walks this queue; budget eviction pops from its front.
    retained_allocations: VecDeque<RetainedAllocation>,
    // Exact sum of `retained_allocations` layout bytes.
    retained_bytes: usize,
    stdout: Vec<u8>,
    print_observer: Option<PrintObserver>,
    print_observer_userdata: *mut c_void,
    trap: Option<TrapRecord>,
    trap_observer: Option<TrapObserver>,
    trap_observer_userdata: *mut c_void,
    trap_observer_active: bool,
    diagnostics_observer: Option<DiagnosticsObserver>,
    diagnostics_observer_userdata: *mut c_void,
    binding_count_advisory_threshold: u64,
    interned: HashMap<(usize, usize), usize>,
    // One ordinary allocated string per distinct astral scalar observed by
    // string `for…of`. Values are permanent collection roots.
    astral_code_points: HashMap<u32, usize>,
    shadow: Vec<(usize, usize)>,
    roots: Vec<(usize, usize)>,
    callbacks: Vec<Box<CallbackBinding>>,
    callback_interns: HashMap<CallbackIdentity, *mut CallbackBinding>,
    // Transient P13 JSON output builders. Untracked serializers create
    // no active-reference set; tracked ones do so explicitly.
    json_builders: crate::json::JsonBuilders,
    // Transient P13 parsed syntax trees. They contain no language
    // allocations and are removed before JSON.parse returns.
    json_parsers: crate::json::JsonParsers,
    // The ship construction path uses the §8.1b arena while freed-handle
    // diagnostics are off. The dev path keeps exact-size individual
    // allocations so §18.2d accounting remains exact.
    ship_arena: bool,
    // §8.1a-3 diagnostic mode. When true, freed allocations at or above
    // `freed_handle_diagnostics_min_payload_bytes` are retained and poisoned
    // within `freed_handle_diagnostics_max_retained_bytes`. Both construction
    // paths default to false.
    freed_handle_diagnostics: bool,
    // Requested-payload boundary for diagnostic retention. This is the same
    // quantity exact-size `live_bytes` sums, not the allocation layout size.
    freed_handle_diagnostics_min_payload_bytes: usize,
    // Hard ceiling on retained layout bytes. Oldest records are evicted to
    // make room for a newly retired allocation.
    freed_handle_diagnostics_max_retained_bytes: usize,
    // The diagnostic setting is immutable after the first object-level
    // allocation request, including one rejected by fault injection.
    allocation_started: bool,
    // The `Math.random` PRNG (stdlib.md §2), default-seeded on every
    // construction path so dev and ship draw the same contract stream.
    rng: crate::math::Rng,
    // The `Date.now` source (stdlib.md §3): `Some` pins the clock
    // (tests, replays); `None` reads the system UTC clock.
    now_override: Option<i64>,
    // The maximum matching work for one budgeted regex search.
    regex_budget: u64,
    regex: crate::regexops::RegexStore,
    // One-shot object-request allocation fault. `Some(n)` refuses the
    // n-th subsequent Context::alloc request; underlying arena chunk
    // allocations are deliberately not counted because their sequence
    // is tier-specific.
    alloc_fail_countdown: Option<u64>,
    // ----- ship-tier arena state (§8.1b); unused in diagnostic mode -----
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

    fn with_tier(ship_arena: bool) -> Box<Context> {
        Box::new(Context {
            trap_flag: 0,
            reload_epoch: 0,
            fn_table: std::ptr::null(),
            globals: std::ptr::null_mut(),
            module_globals: None,
            workers: WorkerSet::default(),
            script_depth: 0,
            async_roots: VecDeque::new(),
            active_async_frames: Vec::new(),
            async_frames: HashMap::new(),
            allocations: HashMap::new(),
            boundary_scratch: Vec::new(),
            dead_allocations: AddressSet::default(),
            retained_allocations: VecDeque::new(),
            retained_bytes: 0,
            stdout: Vec::new(),
            print_observer: None,
            print_observer_userdata: std::ptr::null_mut(),
            trap: None,
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
            callbacks: Vec::new(),
            callback_interns: HashMap::new(),
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
        })
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
        input_payload_size: usize,
        output_payload_size: usize,
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
            input_payload_size,
            output_payload_size,
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
        match unsafe { self.workers.post(worker, payload) } {
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
        crate::worker::materialize_parent(self, receive)
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
                        "worker trapped with {} at position {}: {}",
                        record.kind.rule(),
                        record.pos_id,
                        record.message
                    ),
                    0,
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

    fn uses_ship_arena(&self) -> bool {
        self.ship_arena && !self.freed_handle_diagnostics
    }

    fn retains_freed_payload(&self, payload_size: usize) -> bool {
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
    }

    /// Number of host-to-script calls currently on the stack. A hot
    /// reload is applied only at zero (the frame-boundary rule).
    #[must_use]
    pub fn script_depth(&self) -> u32 {
        self.script_depth
    }

    // ----- poll-driven async roots (Q34) -----

    /// Registers a fresh compiler-generated async frame. Its reference
    /// count starts at one in the frame header's `reserved` word.
    ///
    /// # Safety
    ///
    /// `frame` is a fresh live coroutine allocation with at least eight
    /// payload bytes and belongs to this Context.
    pub unsafe fn async_register(&mut self, frame: *mut u8) {
        if frame.is_null() {
            return;
        }
        // SAFETY: guaranteed by the caller; offset four is the aligned
        // `uint32_t reserved` word in every generated coroutine header.
        unsafe { (frame.add(4) as *mut u32).write(1) };
        self.async_frames.insert(
            frame as usize,
            AsyncFrameMeta {
                created_epoch: self.reload_epoch,
                completion: None,
            },
        );
    }

    /// Increments one held async handle count.
    ///
    /// # Safety
    ///
    /// `frame` is a registered live async frame in this Context.
    pub unsafe fn async_retain(&mut self, frame: *mut u8) {
        if frame.is_null() || !self.async_frames.contains_key(&(frame as usize)) {
            return;
        }
        // SAFETY: guaranteed by the caller.
        let count = unsafe { (frame.add(4) as *mut u32).read() };
        // Static ownership checking prevents an unbounded copy count in a
        // valid program; saturating avoids wrapping into a premature free.
        unsafe { (frame.add(4) as *mut u32).write(count.saturating_add(1)) };
    }

    /// Decrements one held async handle count and frees the frame exactly
    /// when the count reaches zero.
    ///
    /// # Safety
    ///
    /// `frame` is a registered live async frame in this Context and the
    /// caller owns one reference.
    pub unsafe fn async_release(&mut self, frame: *mut u8, pos_id: u32) {
        if frame.is_null() || !self.async_frames.contains_key(&(frame as usize)) {
            return;
        }
        // SAFETY: guaranteed by the caller.
        let slot = unsafe { &mut *(frame.add(4) as *mut u32) };
        if *slot == 0 {
            return;
        }
        *slot -= 1;
        if *slot == 0 {
            self.async_frames.remove(&(frame as usize));
            self.delete(frame as usize, pos_id);
        }
    }

    /// Reads a held handle's count for the emitted-layout conformance test.
    ///
    /// # Safety
    ///
    /// `frame` is a live async frame.
    #[must_use]
    pub unsafe fn async_count(&self, frame: *const u8) -> u32 {
        if frame.is_null() {
            return 0;
        }
        // SAFETY: guaranteed by the caller.
        unsafe { (frame.add(4) as *const u32).read() }
    }

    /// Returns whether a registered frame predates the current reload epoch.
    #[must_use]
    pub fn async_is_stale(&self, frame: *const u8) -> bool {
        self.async_frames
            .get(&(frame as usize))
            .is_some_and(|meta| meta.created_epoch != self.reload_epoch)
    }

    /// Caches the fulfilled representation after the first held await.
    ///
    /// # Safety
    ///
    /// `value` is null when `size == 0`, otherwise it points to `size`
    /// readable bytes for the duration of this call.
    pub unsafe fn async_complete(&mut self, frame: *mut u8, value: *const u8, size: usize) {
        let Some(meta) = self.async_frames.get_mut(&(frame as usize)) else {
            return;
        };
        let bytes = if size == 0 {
            Vec::new()
        } else {
            // SAFETY: guaranteed by the caller.
            unsafe { std::slice::from_raw_parts(value, size) }.to_vec()
        };
        meta.completion = Some(bytes);
    }

    /// Copies a cached fulfilled representation into `out`, returning
    /// `true` when the handle had already completed.
    ///
    /// # Safety
    ///
    /// `out` is null when `size == 0`, otherwise it points to `size`
    /// writable bytes.
    pub unsafe fn async_result(&self, frame: *const u8, out: *mut u8, size: usize) -> bool {
        let Some(bytes) = self
            .async_frames
            .get(&(frame as usize))
            .and_then(|meta| meta.completion.as_ref())
        else {
            return false;
        };
        if size != 0 {
            if bytes.len() != size {
                return false;
            }
            // SAFETY: guaranteed by the caller; the slices do not overlap
            // because cached bytes are Context-owned storage.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, size) };
        }
        true
    }

    /// Runs a newly invoked async root to its first suspension or
    /// completion, registering a suspended root at the back of the
    /// Context's deterministic pending queue.
    ///
    /// # Safety
    ///
    /// `frame` and `resume` must be the matching compiler-generated async
    /// frame and resume function for this Context.
    pub unsafe fn async_kick(&mut self, frame: *mut u8, resume: AsyncResume) {
        if frame.is_null() || self.trapped() {
            return;
        }
        self.active_async_frames.push(frame as usize);
        let ctx = self as *mut Context;
        // SAFETY: guaranteed by the caller; the active root keeps the frame
        // reachable if the generated body explicitly collects.
        let done = unsafe { resume(ctx, frame, std::ptr::null_mut()) };
        self.active_async_frames.pop();
        if done == 0 && !self.trapped() {
            self.async_roots.push_back(AsyncRoot { frame, resume });
        } else if done != 0 {
            // The pending root owns the creator's initial reference.
            unsafe { self.async_release(frame, 0) };
        }
    }

    /// Number of suspended async root invocations.
    #[must_use]
    pub fn async_pending(&self) -> usize {
        self.async_roots.len()
    }

    /// Resumes every root that was pending at call entry exactly once, in
    /// kick order. A root that suspends returns to the back in the same
    /// relative order; a completed root leaves the queue.
    ///
    /// A trap stops the round and preserves the trapping root plus every
    /// not-yet-stepped root. Consequently clearing a reload-staleness trap
    /// and stepping again observes the same stale frame, matching §8.2's
    /// coroutine behavior.
    ///
    /// # Safety
    ///
    /// Every queued callback/frame pair was supplied through
    /// [`Context::async_kick`] and its generated code remains live.
    pub unsafe fn async_step(&mut self) -> usize {
        if self.trapped() || self.async_roots.is_empty() {
            return self.async_roots.len();
        }
        let mut round = std::mem::take(&mut self.async_roots);
        let active_base = self.active_async_frames.len();
        // `round` is temporarily outside `self.async_roots`. Keep every
        // root in the fixed poll set registered for collection, including
        // roots not yet reached when an earlier callback collects.
        self.active_async_frames
            .extend(round.iter().map(|root| root.frame as usize));
        self.enter_script();
        while let Some(root) = round.pop_front() {
            let ctx = self as *mut Context;
            // SAFETY: roots enter the queue only through `async_kick`.
            let done = unsafe { (root.resume)(ctx, root.frame, std::ptr::null_mut()) };
            if self.trapped() {
                self.async_roots.push_back(root);
                self.async_roots.append(&mut round);
                break;
            }
            if done == 0 {
                self.async_roots.push_back(root);
            } else {
                // Queue ownership ends at completion; no collector pass is
                // needed for the decrement that reaches zero.
                unsafe { self.async_release(root.frame, 0) };
            }
        }
        self.active_async_frames.truncate(active_base);
        self.exit_script();
        self.async_roots.len()
    }

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

    /// Delivers `bytes` to the installed print observer without retaining
    /// them. With no observer, appends `bytes` and a trailing newline to
    /// the stdout sink.
    pub fn print_line(&mut self, bytes: &[u8]) {
        if let Some(observer) = self.print_observer {
            let userdata = self.print_observer_userdata;
            // SAFETY: the host supplied the callback and userdata. The
            // callback contract forbids obtaining and using this Context
            // while the exclusive borrow is live.
            unsafe { observer(userdata, bytes.as_ptr(), bytes.len() as u64) };
            return;
        }
        self.stdout.extend_from_slice(bytes);
        self.stdout.push(b'\n');
    }

    /// Installs a host observer for printed lines. Passing `None` clears
    /// the observer and its userdata.
    pub fn set_print_observer(&mut self, observer: Option<PrintObserver>, userdata: *mut c_void) {
        self.print_observer = observer;
        self.print_observer_userdata = if observer.is_some() {
            userdata
        } else {
            std::ptr::null_mut()
        };
    }

    /// Installs the host observer for optional runtime diagnostics
    /// advisories. Passing `None` clears the observer and its userdata.
    pub fn set_diagnostics_observer(
        &mut self,
        observer: Option<DiagnosticsObserver>,
        userdata: *mut c_void,
    ) {
        self.diagnostics_observer = observer;
        self.diagnostics_observer_userdata = if observer.is_some() {
            userdata
        } else {
            std::ptr::null_mut()
        };
    }

    /// Sets the callback-binding count at which newly interned records are
    /// reported through the optional diagnostics observer.
    ///
    /// The threshold is literal: zero advises on the first record. The
    /// default is [`u64::MAX`].
    pub fn set_binding_count_advisory(&mut self, threshold: u64) {
        self.binding_count_advisory_threshold = threshold;
    }

    /// Reports a newly interned callback binding when its resulting count is
    /// at or above the configured threshold.
    fn advise_binding_count(&mut self) {
        let count = u64::try_from(self.callbacks.len()).unwrap_or(u64::MAX);
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
    /// The observer-none branch returns before the binding scan, so the
    /// default path pays neither the scan nor any retained state.
    fn advise_callback_userdata_free(&mut self, payload: usize, pos_id: u32) {
        let Some(observer) = self.diagnostics_observer else {
            return;
        };
        if !self.is_live(payload)
            || !self.callbacks.iter().any(|binding| {
                binding.userdata1 as usize == payload || binding.userdata2 as usize == payload
            })
        {
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

    /// Allocates `size` payload bytes tagged `class_id`; returns the
    /// zeroed payload pointer, or null after recording an
    /// allocation-failure trap. Dev tier: an individual system
    /// allocation tracked in the map. Ship tier: served from the arena
    /// (§8.1b).
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
                marked: false,
            },
        );
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
        Some((
            unsafe { chunk.base.add(bi * chunk.block_size) },
            chunk.class,
        ))
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

    /// Removes Context-lifetime string-intern entries before their ordinary
    /// allocation is retired through the exported delete path.
    fn clear_string_interns_on_delete(&mut self, payload: usize) {
        self.interned.retain(|_, handle| *handle != payload);
        self.astral_code_points
            .retain(|_, handle| *handle != payload);
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
            if class_id == CLASS_STRING {
                self.clear_string_interns_on_delete(payload);
            }
            if class_id == CLASS_REGEX {
                self.regex.remove_value(payload);
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
            if class_id == CLASS_STRING {
                self.clear_string_interns_on_delete(payload);
            }
            if class_id == CLASS_REGEX {
                self.regex.remove_value(payload);
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

    /// Moves one live exact-size allocation into retained-dead storage when
    /// its layout fits the retention budget. Oldest records are evicted until
    /// the new record fits; an individually over-budget layout is released.
    fn retire_dev_allocation(&mut self, payload: usize) -> Option<u32> {
        let allocation = self.allocations.remove(&payload)?;
        // SAFETY: exact live-map membership proves the complete initialized
        // header remains owned by this Context.
        let class_id = unsafe { (allocation.base.add(8) as *const u32).read() };
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
    fn retain_or_release_removed_allocation(
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
    fn release_dev_allocation(&mut self, payload: usize) -> Option<u32> {
        let allocation = self.allocations.remove(&payload)?;
        // SAFETY: exact live-map membership proves the complete initialized
        // header remains owned by this Context.
        let class_id = unsafe { (allocation.base.add(8) as *const u32).read() };
        // SAFETY: `base`/`layout` came from `alloc_zeroed` in `alloc`; the
        // live record was just removed, so this frees it exactly once.
        unsafe { dealloc(allocation.base, allocation.layout) };
        Some(class_id)
    }

    /// Retains or releases one exact-size allocation according to the
    /// diagnostics mode's requested-payload threshold.
    fn dispose_dev_allocation(&mut self, payload: usize) -> Option<u32> {
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
        if self.uses_ship_arena() {
            self.arena_release(payload);
            return;
        }
        let Some(allocation) = self.allocations.get(&payload) else {
            if self.freed_handle_diagnostics {
                if self.dead_allocations.contains(&payload) {
                    self.trap(
                        TrapKind::DoubleDelete,
                        "Context.free of an already-deleted allocation",
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
        let class_id = unsafe { (allocation.base.add(8) as *const u32).read() };
        if matches!(class_id, CLASS_MAP | CLASS_SET) {
            // End the allocation-table borrow before clearing: backing
            // storage is retired through recursive `delete` calls.
            self.clear_container_on_delete(payload);
        }
        if class_id == CLASS_STRING {
            self.clear_string_interns_on_delete(payload);
        }
        let released_class_id = self.dispose_dev_allocation(payload);
        let Some(released_class_id) = released_class_id else {
            self.trap(
                TrapKind::Internal,
                "Map/Set header disappeared while deleting its storage",
                pos_id,
            );
            return;
        };
        // Both class IDs read the same live header, and
        // clearing Map/Set child storage does not mutate the header.
        debug_assert_eq!(released_class_id, class_id);
        if class_id == CLASS_REGEX {
            self.regex.remove_value(payload);
        }
    }

    /// True when `payload` is a live allocation (test/inspection aid).
    /// Ship tier: the exact membership test — chunk range, block grid,
    /// bump watermark, live header — or a large record (§8.1b).
    #[must_use]
    pub fn is_live(&self, payload: usize) -> bool {
        if self.uses_ship_arena() {
            if let Some((block, _)) = self.arena_lookup_block(payload) {
                // SAFETY: `block` heads a block inside an owned chunk.
                return unsafe { (block as *const u64).read() } == LIVE_STATE;
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
            "use of a deleted allocation",
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
        self.trap(
            TrapKind::CallbackUserdataFreed,
            "callback userdata is not a live allocation",
            0,
        );
        false
    }

    /// Number of live allocations (test/inspection aid). Ship tier: a
    /// chunk walk (live blocks below each watermark) plus the large
    /// records.
    #[must_use]
    pub fn live_count(&self) -> usize {
        if self.uses_ship_arena() {
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
        self.allocations.len()
    }

    /// Payload capacity in live allocations.
    ///
    /// The development tier reports exact requested payload sizes. The
    /// ship tier reports size-class payload capacity for arena blocks and
    /// exact payload size for large allocations.
    #[must_use]
    pub fn live_bytes(&self) -> usize {
        if self.uses_ship_arena() {
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
            .fold(0usize, |sum, a| sum.saturating_add(a.payload_size))
    }

    /// Bytes currently reserved from the system for Context allocations.
    ///
    /// Exact-size allocations include only live layouts unless freed-handle
    /// diagnostics retain dead layouts. Ship-tier chunks remain reserved,
    /// while a deleted large allocation is returned to the system
    /// immediately.
    #[must_use]
    pub fn reserved_bytes(&self) -> usize {
        if self.uses_ship_arena() {
            let chunk_bytes = self
                .chunks
                .iter()
                .fold(0usize, |sum, c| sum.saturating_add(c.layout.size()));
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
                    (allocation.base.add(8) as *const u32).read(),
                    (allocation.base.add(12) as *const u32).read(),
                )
            };
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
        work.extend(self.async_roots.iter().map(|root| root.frame as usize));
        work.extend(self.active_async_frames.iter().copied());
        // Counted frames are roots even when the only holders form a cycle.
        // This deliberately makes such a cycle leak (§70.3.6); collection
        // must not reinterpret the reference-count ownership model.
        work.extend(self.async_frames.keys().copied());
        for completion in self
            .async_frames
            .values()
            .filter_map(|meta| meta.completion.as_deref())
        {
            for word in completion.chunks_exact(core::mem::size_of::<usize>()) {
                // SAFETY: `word` is exactly one native word of Context-owned
                // completion storage; unaligned reads preserve aggregate
                // layouts while exposing any managed handle to the marker.
                work.push(unsafe { word.as_ptr().cast::<usize>().read_unaligned() });
            }
        }
        work.extend(self.interned.values().copied());
        work.extend(self.astral_code_points.values().copied());
        for binding in &self.callbacks {
            for userdata in [binding.userdata1, binding.userdata2] {
                let address = userdata as usize;
                if !userdata.is_null() && self.is_live(address) {
                    work.push(address);
                }
            }
        }

        if self.uses_ship_arena() {
            // Ship tier (§8.1b): mark state lives in the block header
            // (MARK_STATE), not in a map; sweep walks the chunk grids and
            // the large records.
            self.arena_mark(&mut work);
            self.arena_sweep();
            self.sweep_regex_values();
            return;
        }

        let mut marked_count = 0usize;
        while let Some(addr) = work.pop() {
            let Some(a) = self.allocations.get_mut(&addr) else {
                continue;
            };
            if a.marked {
                continue;
            }
            a.marked = true;
            marked_count += 1;
            let payload = addr as *const u8;
            let words = a.payload_size / 8;
            for i in 0..words {
                // SAFETY: the payload is owned by this context and at
                // least `payload_size` bytes; reads stay inside it.
                let w = unsafe { (payload.add(i * 8) as *const usize).read_unaligned() };
                work.push(w);
            }
        }

        self.sweep_dev_allocations(self.allocations.len() - marked_count);
        self.sweep_regex_values();
    }

    /// Exact-size allocator sweep: extract unreachable records from the
    /// only map this phase walks and reset marked survivors in the same
    /// pass.
    ///
    /// The retained-dead address index and ownership queue are deliberately
    /// never traversed here.
    fn sweep_dev_allocations(&mut self, retiring: usize) {
        if !self.freed_handle_diagnostics {
            for (_, allocation) in self.allocations.extract_if(|_, allocation| {
                if allocation.marked {
                    allocation.marked = false;
                    false
                } else {
                    true
                }
            }) {
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
        // `extract_if` retains the live map's bucket storage. Later bursts
        // reuse it; accumulated deletion tombstones can eventually force one
        // bounded rebuild, but dead-count growth no longer repeats peak
        // rehashes.
        for (addr, allocation) in self.allocations.extract_if(|_, allocation| {
            if allocation.marked {
                allocation.marked = false;
                false
            } else {
                true
            }
        }) {
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
    fn sweep_regex_values(&mut self) {
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
    /// is stamped `MARK_STATE` and its payload words are pushed.
    fn arena_mark(&mut self, work: &mut Vec<usize>) {
        while let Some(addr) = work.pop() {
            let (block, payload_size) = if let Some((block, class)) = self.arena_lookup_block(addr)
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

    /// Returns the allocation-free string handle for one BMP scalar.
    ///
    /// The odd tagged value is never dereferenced directly. Its UTF-8
    /// bytes live in [`CODE_POINT_UTF8`] and therefore remain valid for
    /// every Context and across storage in arrays, maps, and fields.
    #[must_use]
    fn inline_bmp_code_point(value: char) -> *mut u8 {
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

    /// Registers a C-callback binding and returns a stable pointer to it
    /// (P5.2b). The pointer is what a boundary marshaler stores in a C
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
    pub fn bind_callback(
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

    /// Allocates a byte array and copies `len` bytes from `src`.
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
        let handle = self.array_new(1, pos_id);
        if handle.is_null() || len == 0 {
            return handle;
        }
        let data = self.alloc(len, CLASS_ARRAY_DATA, pos_id);
        if data.is_null() {
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
                if self.uses_ship_arena() {
                    // Ship tier (§8.1b): retired data blocks flow through
                    // the same free-list/large-record release path as
                    // `delete`, so array growth does not accumulate.
                    self.arena_release(old);
                } else {
                    let disposed = self.dispose_dev_allocation(old);
                    match disposed {
                        Some(CLASS_ARRAY_DATA) => {}
                        Some(_) => {
                            self.trap(
                                TrapKind::Internal,
                                "array growth found a non-array storage allocation",
                                pos_id,
                            );
                            return -1;
                        }
                        None => {
                            self.trap(
                                TrapKind::Internal,
                                "array storage disappeared while growing it",
                                pos_id,
                            );
                            return -1;
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
        // Closing before joining wakes children blocked in inbox waits. The
        // worker threads release their own Contexts before join completes.
        self.workers.shutdown();
        self.boundary_scratch_release(0);
        if let Some(block) = self.module_globals.take() {
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `init_module_globals` and are freed exactly once, here.
            unsafe { dealloc(block.base, block.layout) };
        }
        for a in self.allocations.values() {
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `Context::alloc` and are freed exactly once, here.
            unsafe { dealloc(a.base, a.layout) };
        }
        for a in &self.retained_allocations {
            // SAFETY: retained records own allocations not present in the
            // live map; any evicted records were already popped and freed.
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
            .field("dead_allocations", &self.dead_allocations.len())
            .field("stdout_len", &self.stdout.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct TestAsyncFrame {
        id: u8,
        polls: u8,
    }

    #[repr(C)]
    struct TestCountedAsyncFrame {
        state: i32,
        count: u32,
        resume: AsyncResume,
    }

    unsafe extern "C" fn counted_test_resume(
        _ctx: *mut Context,
        _frame: *mut u8,
        _out: *mut u8,
    ) -> u8 {
        1
    }

    unsafe extern "C" fn test_async_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
        // SAFETY: the tests pass matching live `TestAsyncFrame` values.
        let ctx = unsafe { &mut *ctx };
        let frame = unsafe { &mut *frame.cast::<TestAsyncFrame>() };
        ctx.print_line(if frame.id == 1 { b"one" } else { b"two" });
        frame.polls += 1;
        if frame.id == 3 && frame.polls == 2 {
            ctx.collect();
        }
        u8::from(frame.polls == 2)
    }

    #[test]
    fn held_async_count_uses_emitted_header_and_frees_without_collect() {
        assert_eq!(core::mem::offset_of!(TestCountedAsyncFrame, state), 0);
        assert_eq!(core::mem::offset_of!(TestCountedAsyncFrame, count), 4);
        assert_eq!(core::mem::offset_of!(TestCountedAsyncFrame, resume), 8);
        assert_eq!(core::mem::size_of::<TestCountedAsyncFrame>(), 16);

        let mut ctx = Context::new();
        let frame = ctx.alloc(
            core::mem::size_of::<TestCountedAsyncFrame>(),
            CLASS_GENERATOR,
            70,
        );
        assert!(!frame.is_null());
        // SAFETY: the allocation has exactly the emitted prefix layout.
        unsafe {
            frame
                .cast::<TestCountedAsyncFrame>()
                .write(TestCountedAsyncFrame {
                    state: 0,
                    count: 0,
                    resume: counted_test_resume,
                });
            ctx.async_register(frame);
            assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 1);
            assert_eq!(ctx.async_count(frame), 1);

            // Compiler-emitted copy retain.
            ctx.async_retain(frame);
            assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 2);

            // Compiler-emitted inner-scope exit release.
            ctx.async_release(frame, 70);
            assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 1);

            // Await caches/reads completion but does not change ownership.
            let fulfilled = 37i32;
            ctx.async_complete(frame, (&fulfilled as *const i32).cast(), 4);
            let mut observed = 0i32;
            assert!(ctx.async_result(frame, (&mut observed as *mut i32).cast(), 4));
            assert_eq!(observed, fulfilled);
            assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 1);

            // The final lexical decrement frees immediately. No collect call
            // occurs anywhere in this test.
            ctx.async_release(frame, 70);
        }
        assert!(!ctx.is_live(frame as usize));
        assert_eq!(ctx.live_bytes(), 0);
    }

    #[test]
    fn async_step_resumes_pending_roots_in_kick_order() {
        let mut ctx = Context::new();
        let mut one = TestAsyncFrame { id: 1, polls: 0 };
        let mut two = TestAsyncFrame { id: 2, polls: 0 };
        // SAFETY: both frames remain live until their second poll completes.
        unsafe {
            ctx.async_kick((&mut one as *mut TestAsyncFrame).cast(), test_async_resume);
            ctx.async_kick((&mut two as *mut TestAsyncFrame).cast(), test_async_resume);
        }
        assert_eq!(ctx.async_pending(), 2);
        assert_eq!(ctx.take_stdout(), b"one\ntwo\n");
        // SAFETY: the queued callbacks and frames are still valid.
        assert_eq!(unsafe { ctx.async_step() }, 0);
        assert_eq!(ctx.take_stdout(), b"one\ntwo\n");
    }

    #[test]
    fn async_step_on_trapped_context_is_no_op() {
        let mut ctx = Context::new();
        let mut frame = TestAsyncFrame { id: 1, polls: 0 };
        // SAFETY: `frame` remains live for the test.
        unsafe {
            ctx.async_kick(
                (&mut frame as *mut TestAsyncFrame).cast(),
                test_async_resume,
            )
        };
        ctx.trap(TrapKind::Internal, "test trap", 7);
        // SAFETY: the queued callback and frame remain valid, but the trap
        // contract prevents the callback from being invoked.
        assert_eq!(unsafe { ctx.async_step() }, 1);
        assert_eq!(frame.polls, 1);
        assert_eq!(ctx.async_pending(), 1);
    }

    #[test]
    fn dropping_context_does_not_resume_suspended_async_roots() {
        let mut frame = TestAsyncFrame { id: 1, polls: 0 };
        {
            let mut ctx = Context::new();
            // SAFETY: `frame` outlives the Context and its pending queue.
            unsafe {
                ctx.async_kick(
                    (&mut frame as *mut TestAsyncFrame).cast(),
                    test_async_resume,
                )
            };
            assert_eq!(ctx.async_pending(), 1);
        }
        assert_eq!(frame.polls, 1, "teardown must not run a continuation");
    }

    #[test]
    fn async_step_keeps_unstepped_roots_live_during_collection() {
        let mut ctx = Context::new();
        let first = ctx.alloc(std::mem::size_of::<TestAsyncFrame>(), CLASS_GENERATOR, 0);
        let second = ctx.alloc(std::mem::size_of::<TestAsyncFrame>(), CLASS_GENERATOR, 0);
        // SAFETY: both allocations have exactly the test-frame payload and
        // remain Context-owned throughout the poll round.
        unsafe {
            first
                .cast::<TestAsyncFrame>()
                .write(TestAsyncFrame { id: 3, polls: 0 });
            second
                .cast::<TestAsyncFrame>()
                .write(TestAsyncFrame { id: 2, polls: 0 });
            ctx.async_kick(first, test_async_resume);
            ctx.async_kick(second, test_async_resume);
            assert_eq!(ctx.async_step(), 0);
        }
        assert!(
            ctx.is_live(second as usize),
            "the first root's collect must retain roots not yet polled"
        );
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
        assert!(
            crate::date::in_range(a),
            "system clock out of TimeClip: {a}"
        );
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
        assert_eq!(unsafe { crate::ffi::subscript_rt_ctx_clear_trap(p) }, 0);
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
    fn ship_module_globals_are_context_owned_zeroed_and_reused() {
        let mut ctx = Context::new_releasing();
        let first = ctx.init_module_globals(24, 16);
        assert!(!first.is_null());
        assert_eq!(first as usize % 16, 0);
        assert_eq!(ctx.globals, first);
        // SAFETY: the Context owns 24 writable bytes at `first`.
        unsafe { std::ptr::write_bytes(first, 0xA5, 24) };

        let second = ctx.init_module_globals(24, 16);
        assert_eq!(second, first, "same image must reuse its Context block");
        // SAFETY: the reused block still owns 24 readable bytes.
        let bytes = unsafe { std::slice::from_raw_parts(second, 24) };
        assert_eq!(bytes, &[0; 24]);
    }

    #[test]
    fn print_observer_controls_delivery_and_sink_retention() {
        unsafe extern "C" fn observe(userdata: *mut c_void, line: *const u8, line_len: u64) {
            // SAFETY: the test passes a live Vec with this exact type and
            // the observer contract keeps the line readable for this call.
            let lines = unsafe { &mut *userdata.cast::<Vec<Vec<u8>>>() };
            // SAFETY: `line` addresses `line_len` readable bytes for this
            // callback.
            let line = unsafe { std::slice::from_raw_parts(line, line_len as usize) };
            lines.push(line.to_vec());
        }

        let mut unset = Context::new();
        unset.print_line(b"default");
        assert_eq!(unset.take_stdout(), b"default\n");

        let mut ctx = Context::new();
        let mut observed = Vec::<Vec<u8>>::new();
        ctx.set_print_observer(Some(observe), std::ptr::from_mut(&mut observed).cast());
        ctx.print_line(b"first");
        ctx.print_line(b"second");
        assert_eq!(observed, [b"first".to_vec(), b"second".to_vec()]);
        assert!(ctx.stdout_bytes().is_empty());

        ctx.set_print_observer(None, std::ptr::from_mut(&mut observed).cast());
        ctx.print_line(b"after-unset");
        assert_eq!(observed, [b"first".to_vec(), b"second".to_vec()]);
        assert_eq!(ctx.stdout_bytes(), b"after-unset\n");
        assert!(ctx.print_observer_userdata.is_null());
    }

    #[test]
    fn callback_bindings_are_interned_by_identity() {
        fn first_code() {}

        let mut ctx = Context::new();
        let code = first_code as *const () as *const u8;
        let mut userdata1 = 1u8;
        let mut userdata2 = 2u8;
        let mut other_userdata2 = 3u8;
        let userdata1 = std::ptr::from_mut(&mut userdata1);
        let userdata2 = std::ptr::from_mut(&mut userdata2);
        let other_userdata2 = std::ptr::from_mut(&mut other_userdata2);

        let first = ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2);
        let repeated = ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2);
        assert_eq!(first, repeated);
        assert_eq!(ctx.callbacks.len(), 1);

        let second = ctx.bind_callback(code, std::ptr::null(), userdata1, other_userdata2);
        assert_ne!(first, second);
        assert_eq!(ctx.callbacks.len(), 2);
    }

    #[test]
    fn binding_count_advisory_reports_distinct_identity_at_threshold() {
        fn callback_code() {}

        #[derive(Default)]
        struct Advisories(Vec<(u32, u32, Vec<u8>)>);

        unsafe extern "C" fn observe(
            userdata: *mut c_void,
            kind: u32,
            pos_id: u32,
            message: *const u8,
            message_len: u64,
        ) {
            // SAFETY: the test passes a live Advisories value, and the
            // observer contract supplies readable message bytes.
            let observed = unsafe { &mut *userdata.cast::<Advisories>() };
            // SAFETY: the message remains readable for this callback.
            let message =
                unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
            observed.0.push((kind, pos_id, message));
        }

        let mut ctx = Context::new();
        let mut observed = Advisories::default();
        ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut observed).cast());
        ctx.set_binding_count_advisory(2);
        let code = callback_code as *const () as *const u8;
        let mut first_userdata = 1u8;
        let mut second_userdata = 2u8;

        ctx.bind_callback(
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut first_userdata),
            std::ptr::null_mut(),
        );
        assert!(observed.0.is_empty(), "below-threshold binding advised");

        ctx.bind_callback(
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut second_userdata),
            std::ptr::null_mut(),
        );
        assert_eq!(
            observed.0,
            [(
                DIAGNOSTICS_ADVISORY_BINDING_COUNT,
                0,
                b"callback bindings: 2 registered, advisory threshold 2".to_vec(),
            )]
        );

        let mut zero_ctx = Context::new();
        let mut zero_observed = Advisories::default();
        zero_ctx
            .set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut zero_observed).cast());
        zero_ctx.set_binding_count_advisory(0);
        zero_ctx.bind_callback(
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut first_userdata),
            std::ptr::null_mut(),
        );
        assert_eq!(
            zero_observed.0,
            [(
                DIAGNOSTICS_ADVISORY_BINDING_COUNT,
                0,
                b"callback bindings: 1 registered, advisory threshold 0".to_vec(),
            )],
            "zero must be a literal first-record threshold"
        );
    }

    #[test]
    fn binding_count_advisory_skips_same_identity_reregistration_at_threshold() {
        fn callback_code() {}

        unsafe extern "C" fn observe(
            userdata: *mut c_void,
            _kind: u32,
            _pos_id: u32,
            _message: *const u8,
            _message_len: u64,
        ) {
            // SAFETY: the test passes a live counter.
            unsafe { *userdata.cast::<u32>() += 1 };
        }

        let mut ctx = Context::new();
        let code = callback_code as *const () as *const u8;
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);
        let first = ctx.bind_callback(code, std::ptr::null(), userdata, std::ptr::null_mut());

        let mut calls = 0u32;
        ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut calls).cast());
        ctx.set_binding_count_advisory(1);
        let repeated = ctx.bind_callback(code, std::ptr::null(), userdata, std::ptr::null_mut());

        assert_eq!(first, repeated);
        assert_eq!(ctx.callbacks.len(), 1);
        assert_eq!(calls, 0, "an intern hit must never advise");
    }

    #[test]
    fn callback_reregistration_has_zero_record_growth_at_frame_scale() {
        fn callback_code() {}

        let mut ctx = Context::new();
        let code = callback_code as *const () as *const u8;
        let mut userdata1 = 1u8;
        let mut userdata2 = 2u8;
        let userdata1 = std::ptr::from_mut(&mut userdata1);
        let userdata2 = std::ptr::from_mut(&mut userdata2);
        let first = ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2);

        for _ in 1..10_000 {
            assert_eq!(
                ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2),
                first
            );
        }

        assert_eq!(
            ctx.callbacks.len(),
            1,
            "10,000 registrations of one identity must retain one record"
        );
    }

    #[test]
    fn callback_userdata_rooted_survives_collect() {
        fn callback_code() {}

        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let first = ctx.alloc(16, 1, 10);
            let second = ctx.alloc(16, 2, 11);
            assert!(!first.is_null() && !second.is_null(), "{tier}");
            ctx.bind_callback(
                callback_code as *const () as *const u8,
                std::ptr::null(),
                first,
                second,
            );

            ctx.collect();

            assert!(ctx.is_live(first as usize), "{tier}: first userdata");
            assert!(ctx.is_live(second as usize), "{tier}: second userdata");
            assert_eq!(ctx.live_count(), 2, "{tier}: rooted accounting");
        }
    }

    #[test]
    fn callback_userdata_freed_slot_is_skipped_at_mark() {
        fn callback_code() {}

        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
            let freed = ctx.alloc(16, 1, 12);
            let unrooted = ctx.alloc(16, 2, 13);
            assert!(!freed.is_null() && !unrooted.is_null(), "{tier}");
            ctx.bind_callback(
                callback_code as *const () as *const u8,
                std::ptr::null(),
                freed,
                std::ptr::null_mut(),
            );
            ctx.delete(freed as usize, 14);

            ctx.collect();

            assert!(!ctx.trapped(), "{tier}: mark must skip the dead slot");
            assert!(!ctx.is_live(freed as usize), "{tier}: freed slot");
            assert!(
                !ctx.is_live(unrooted as usize),
                "{tier}: unrooted allocation"
            );
            assert_eq!(ctx.live_count(), 0, "{tier}: live accounting");
        }
    }

    #[test]
    fn diagnostics_observer_advises_on_callback_userdata_free() {
        fn callback_code() {}

        #[derive(Debug, Default, PartialEq, Eq)]
        struct Advisory {
            kind: u32,
            pos_id: u32,
            message: Vec<u8>,
        }

        unsafe extern "C" fn observe(
            userdata: *mut c_void,
            kind: u32,
            pos_id: u32,
            message: *const u8,
            message_len: u64,
        ) {
            // SAFETY: the test passes a live Advisory and the callback
            // contract supplies `message_len` readable bytes.
            let advisory = unsafe { &mut *userdata.cast::<Advisory>() };
            advisory.kind = kind;
            advisory.pos_id = pos_id;
            // SAFETY: the observer contract keeps the message readable for
            // the duration of this call.
            advisory.message =
                unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
        }

        let mut ctx = Context::new();
        let registered = ctx.alloc(16, 1, 15);
        assert!(!registered.is_null());
        ctx.bind_callback(
            callback_code as *const () as *const u8,
            std::ptr::null(),
            std::ptr::null_mut(),
            registered,
        );
        let mut advisory = Advisory::default();
        ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut advisory).cast());

        ctx.delete(registered as usize, 91);

        assert_eq!(
            advisory,
            Advisory {
                kind: DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE,
                pos_id: 91,
                message: b"Context.free of registered callback userdata".to_vec(),
            }
        );
        assert!(!ctx.is_live(registered as usize));
        assert!(!ctx.trapped(), "an advisory must not become a trap");
    }

    #[test]
    fn diagnostics_observer_unset_has_zero_change() {
        fn callback_code() {}

        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            assert!(ctx.diagnostics_observer.is_none(), "{tier}");
            assert!(ctx.diagnostics_observer_userdata.is_null(), "{tier}");
            assert_eq!(
                ctx.binding_count_advisory_threshold,
                u64::MAX,
                "{tier}: binding advisory default changed"
            );
            let registered = ctx.alloc(16, 1, 16);
            assert!(!registered.is_null(), "{tier}");
            ctx.bind_callback(
                callback_code as *const () as *const u8,
                std::ptr::null(),
                registered,
                std::ptr::null_mut(),
            );

            ctx.delete(registered as usize, 92);

            assert!(!ctx.is_live(registered as usize), "{tier}");
            assert!(!ctx.trapped(), "{tier}: default free behavior changed");
            assert!(
                ctx.dead_allocations.is_empty(),
                "{tier}: free retained memory"
            );
        }
    }

    #[test]
    fn alloc_is_zeroed_tagged_and_live() {
        let mut ctx = Context::new();
        let p = ctx.alloc(24, 3, 41);
        assert!(!p.is_null());
        assert!(ctx.is_live(p as usize));
        // SAFETY: p is a fresh 24-byte payload with a 16-byte header.
        unsafe {
            assert_eq!(
                (p.offset(STATE_OFFSET as isize) as *const u64).read(),
                LIVE_STATE
            );
            assert_eq!((p.offset(CLASS_ID_OFFSET as isize) as *const u32).read(), 3);
            assert_eq!((p.offset(POS_ID_OFFSET as isize) as *const u32).read(), 41);
            for i in 0..24 {
                assert_eq!(p.add(i).read(), 0);
            }
        }
    }

    #[test]
    fn allocation_fault_counts_object_requests_identically_in_both_tiers() {
        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
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
            let triples = unsafe { &mut *userdata.cast::<Vec<(u32, u32, u64)>>() };
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
        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let first = ctx.alloc(1, 1, 0);
            let deleted = ctx.alloc(17, 1, 0);
            let large = ctx.alloc(5000, 1, 0);
            assert!(!first.is_null() && !deleted.is_null() && !large.is_null());
            assert_eq!(ctx.live_count(), 3, "{tier}: N allocations");
            let reserved_before = ctx.reserved_bytes();

            ctx.delete(deleted as usize, 0);
            assert_eq!(ctx.live_count(), 2, "{tier}: N-M allocations");
            if tier == "dev" {
                assert!(
                    ctx.reserved_bytes() < reserved_before,
                    "dev default must release the exact-size layout"
                );
            } else {
                assert_eq!(
                    ctx.reserved_bytes(),
                    reserved_before,
                    "ship size-class storage stays reserved in its reusable arena"
                );
            }
            measured.push((
                tier,
                ctx.live_count(),
                ctx.live_bytes(),
                ctx.reserved_bytes(),
            ));
        }

        assert_eq!(measured[0], ("dev", 2, 5001, 5033));
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
    fn freed_handle_diagnostics_setting_controls_release_and_retention() {
        let mut releasing = Context::new();
        assert!(releasing.set_freed_handle_diagnostics(false, usize::MAX, 0));
        let released_small = releasing.alloc(8, 1, 0);
        releasing.delete(released_small as usize, 0);
        let released_after_small = releasing.reserved_bytes();
        let released_large = releasing.alloc(8, 1, 0);
        releasing.delete(released_large as usize, 0);
        assert_eq!(releasing.live_bytes(), 0);
        assert_eq!(
            releasing.reserved_bytes(),
            released_after_small,
            "mode off must not retain each freed allocation"
        );
        assert!(
            !releasing.set_freed_handle_diagnostics(true, 0, usize::MAX),
            "the setting is immutable after allocation starts"
        );

        let mut diagnosing = Context::new();
        assert!(diagnosing.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let retained_small = diagnosing.alloc(8, 1, 0);
        diagnosing.delete(retained_small as usize, 0);
        let retained_after_small = diagnosing.reserved_bytes();
        let retained_large = diagnosing.alloc(8, 1, 0);
        diagnosing.delete(retained_large as usize, 0);
        assert_eq!(diagnosing.live_bytes(), 0);
        assert!(
            diagnosing.reserved_bytes() > retained_after_small,
            "mode on must retain each freed allocation"
        );

        diagnosing.delete(retained_large as usize, 7);
        assert_eq!(
            diagnosing.trap_record().map(|record| record.kind),
            Some(TrapKind::DoubleDelete),
            "mode on must preserve the diagnostic path"
        );
    }

    #[test]
    fn freed_handle_diagnostics_threshold_controls_delete_and_collect_accounting() {
        for collect in [false, true] {
            let mut ctx = Context::new();
            assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));

            let below = ctx.alloc(12, 1, 0);
            assert_eq!(ctx.live_bytes(), 12);
            assert_eq!(ctx.reserved_bytes(), 12 + HEADER_SIZE);
            if collect {
                ctx.collect();
            } else {
                ctx.delete(below as usize, 0);
            }
            assert_eq!(ctx.live_bytes(), 0);
            assert_eq!(
                ctx.reserved_bytes(),
                0,
                "payload below the threshold must be released"
            );

            let at = ctx.alloc(32, 1, 0);
            if collect {
                ctx.collect();
            } else {
                ctx.delete(at as usize, 0);
            }
            assert_eq!(ctx.live_bytes(), 0);
            assert_eq!(
                ctx.reserved_bytes(),
                32 + HEADER_SIZE,
                "payload at the threshold must be retained"
            );

            let above = ctx.alloc(128, 1, 0);
            if collect {
                ctx.collect();
            } else {
                ctx.delete(above as usize, 0);
            }
            assert_eq!(ctx.live_bytes(), 0);
            assert_eq!(
                ctx.reserved_bytes(),
                32 + HEADER_SIZE + 128 + HEADER_SIZE,
                "payload above the threshold must be retained"
            );
            assert_eq!(ctx.dead_allocations.len(), 2);
            assert_eq!(ctx.retained_allocations.len(), 2);
        }
    }

    #[test]
    fn retention_budget_never_exceeds_and_evicts_oldest_first() {
        const BUDGET: usize = 72;
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, BUDGET));

        let oldest = ctx.alloc(32, 1, 0);
        ctx.delete(oldest as usize, 0);
        assert_eq!(ctx.retained_bytes, 48);
        assert_eq!(ctx.reserved_bytes(), 48);

        let middle = ctx.alloc(8, 1, 0);
        ctx.delete(middle as usize, 0);
        assert_eq!(ctx.retained_bytes, BUDGET);
        assert_eq!(ctx.reserved_bytes(), BUDGET);

        let newest = ctx.alloc(8, 1, 0);
        ctx.delete(newest as usize, 0);
        assert!(ctx.retained_bytes <= BUDGET);
        assert_eq!(ctx.retained_bytes, 48);
        assert_eq!(
            ctx.reserved_bytes(),
            48,
            "evicting the older larger layout must lower reserved accounting"
        );
        assert!(!ctx.dead_allocations.contains(&(oldest as usize)));
        assert!(ctx.dead_allocations.contains(&(middle as usize)));
        assert!(ctx.dead_allocations.contains(&(newest as usize)));
        // SAFETY: newest remains retained and owned by the Context.
        unsafe {
            assert_eq!(
                (newest.offset(STATE_OFFSET as isize) as *const u64).read(),
                DEAD_STATE
            );
        }
        assert_eq!(
            ctx.retained_allocations
                .iter()
                .map(|allocation| allocation.payload)
                .collect::<Vec<_>>(),
            [middle as usize, newest as usize],
            "the oldest retirement must be evicted first"
        );

        // No allocation has occurred since `oldest` was evicted, so its
        // released address is still in the deterministic best-effort window.
        assert!(!ctx.require_live_handle(oldest as usize, 31));
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::UseAfterDelete)
        );
        ctx.clear_trap();

        // The newest free remains retained and therefore has the guaranteed
        // diagnostic coverage funded by the budget.
        assert!(!ctx.require_live_handle(newest as usize, 32));
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::UseAfterDelete)
        );
    }

    #[test]
    fn retention_budget_applies_to_collect_sweeps() {
        const BUDGET: usize = 48;
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, BUDGET));

        for _ in 0..3 {
            assert!(!ctx.alloc(8, 1, 0).is_null());
            ctx.collect();
            assert!(ctx.retained_bytes <= BUDGET);
            assert!(ctx.reserved_bytes() <= BUDGET);
        }
        assert_eq!(ctx.retained_bytes, BUDGET);
        assert_eq!(ctx.retained_allocations.len(), 2);
        assert_eq!(ctx.dead_allocations.len(), 2);
    }

    #[test]
    fn allocation_larger_than_retention_budget_is_released_immediately() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, 47));
        let allocation = ctx.alloc(32, 1, 0);
        ctx.delete(allocation as usize, 0);

        assert_eq!(ctx.retained_bytes, 0);
        assert_eq!(ctx.reserved_bytes(), 0);
        assert!(ctx.retained_allocations.is_empty());
        assert!(!ctx.dead_allocations.contains(&(allocation as usize)));
        assert!(!ctx.require_live_handle(allocation as usize, 41));
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::UseAfterDelete)
        );
    }

    #[test]
    fn zero_retention_budget_retains_nothing() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, 0));
        let allocation = ctx.alloc(8, 1, 0);
        ctx.delete(allocation as usize, 0);

        assert_eq!(ctx.retained_bytes, 0);
        assert_eq!(ctx.reserved_bytes(), 0);
        assert!(ctx.retained_allocations.is_empty());
        assert!(ctx.dead_allocations.is_empty());
    }

    #[test]
    fn array_growth_applies_diagnostics_threshold_to_retired_backing_payloads() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
        let array = ctx.array_new(4, 0);
        for value in 0..9i32 {
            // SAFETY: `array` is a live i32 array and `value` supplies one
            // initialized element for the duration of the call.
            assert!(unsafe { ctx.array_push(array, (&value as *const i32).cast(), 0) } > 0);
        }

        assert_eq!(ctx.dead_allocations.len(), 1);
        assert_eq!(ctx.retained_allocations.len(), 1);
        assert_eq!(
            ctx.retained_allocations[0].layout.size() - HEADER_SIZE,
            32,
            "the retired 16-byte backing store is released and the 32-byte store is retained"
        );
    }

    #[test]
    fn freed_handle_diagnostics_setting_refuses_changes_after_allocation_starts() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
        let below = ctx.alloc(12, 1, 0);
        assert!(
            !ctx.set_freed_handle_diagnostics(true, 0, usize::MAX),
            "mode and threshold must be immutable after allocation starts"
        );
        ctx.delete(below as usize, 0);
        assert_eq!(
            ctx.reserved_bytes(),
            0,
            "a refused change must leave the original threshold in force"
        );
    }

    #[test]
    fn freed_handle_diagnostics_threshold_and_budget_are_ignored_when_disabled() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(false, usize::MAX, 0));
        let allocation = ctx.alloc(128, 1, 0);
        ctx.delete(allocation as usize, 0);
        assert_eq!(ctx.live_bytes(), 0);
        assert_eq!(ctx.reserved_bytes(), 0);
        assert!(ctx.dead_allocations.is_empty());
        assert!(ctx.retained_allocations.is_empty());
    }

    #[test]
    fn below_threshold_stale_handle_traps_before_address_reuse() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
        let below = ctx.alloc(12, 1, 0);
        ctx.delete(below as usize, 0);
        assert_eq!(ctx.reserved_bytes(), 0);

        assert!(!ctx.require_live_handle(below as usize, 19));
        assert_eq!(
            ctx.trap_record().map(|record| (record.kind, record.pos_id)),
            Some((TrapKind::UseAfterDelete, 19))
        );
    }

    #[test]
    fn delete_poisons_and_double_delete_traps() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let p = ctx.alloc(8, 1, 0);
        ctx.delete(p as usize, 5);
        assert!(!ctx.is_live(p as usize));
        // SAFETY: diagnostic mode retains the bytes after delete.
        unsafe {
            assert_eq!(
                (p.offset(STATE_OFFSET as isize) as *const u64).read(),
                DEAD_STATE
            );
        }
        assert!(!ctx.trapped());
        ctx.delete(p as usize, 6);
        assert!(ctx.trapped());
        let r = ctx.trap_record().expect("trap recorded");
        assert_eq!(r.kind, TrapKind::DoubleDelete);
        assert_eq!(r.pos_id, 6);
    }

    #[test]
    fn retained_dead_handles_trap_after_700_000_subsequent_allocations() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let oldest = ctx.alloc(8, 1, 1);
        ctx.delete(oldest as usize, 2);
        let mut probes = vec![(oldest, 700_000usize)];

        for i in 0..700_000usize {
            let handle = ctx.alloc(8, 1, 3);
            ctx.delete(handle as usize, 4);
            let distance = 699_999 - i;
            if matches!(distance, 0 | 1 | 1_000) {
                probes.push((handle, distance));
            }
        }

        assert_eq!(ctx.allocations.len(), 0);
        assert_eq!(ctx.dead_allocations.len(), 700_001);
        assert_eq!(ctx.retained_allocations.len(), 700_001);
        for (handle, distance) in probes {
            // The generated-code path reads this same retained header;
            // the runtime receiver path additionally proves segregation
            // still classifies every distance as stale.
            // SAFETY: diagnostic retain-and-poison owns the header through drop.
            unsafe {
                assert_eq!(
                    (handle.offset(STATE_OFFSET as isize) as *const u64).read(),
                    DEAD_STATE,
                    "distance {distance}"
                );
            }
            assert!(
                !ctx.require_live_handle(handle as usize, 91),
                "distance {distance}"
            );
            let trap = ctx.trap_record().expect("use-after-delete trap");
            assert_eq!(trap.kind, TrapKind::UseAfterDelete, "distance {distance}");
            assert_eq!(
                trap.message, "use of a deleted allocation",
                "distance {distance}"
            );
            assert_eq!(trap.pos_id, 91, "distance {distance}");
            ctx.clear_trap();
        }
    }

    #[test]
    fn ordinary_delete_skips_container_path_and_ship_has_one_membership_lookup() {
        use std::sync::atomic::Ordering::SeqCst;

        for mut ctx in [Context::new(), Context::new_releasing()] {
            let uses_ship_arena = ctx.uses_ship_arena();
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
                usize::from(uses_ship_arena),
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
        assert_eq!(
            ctx.allocation_count(),
            1,
            "ship mode leaves no entry behind"
        );

        // A second delete of the now-released pointer does NOT trap
        // (undefined-but-safe no-op, §8.1b), unlike the diagnostic-mode
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

        // Contrast: an equivalent diagnostic-mode delete retains the entry.
        let mut dev = Context::new();
        assert!(dev.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let d0 = dev.alloc(8, 1, 0);
        let _d1 = dev.alloc(8, 1, 0);
        assert_eq!(dev.allocation_count(), 2);
        dev.delete(d0 as usize, 0);
        assert_eq!(
            dev.allocation_count(),
            2,
            "diagnostic mode retains the poisoned entry"
        );
        // `b` and `c` keep the ship context's live set non-trivial.
        assert!(ctx.is_live(b as usize));
    }

    #[test]
    fn delete_of_unowned_pointer_traps_with_nonzero_diagnostics_threshold() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
        ctx.delete(0x1000, 1);
        assert_eq!(
            ctx.trap_record().map(|r| r.kind),
            Some(TrapKind::InvalidDelete)
        );
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
        assert!(
            !ctx.is_live(deleted as usize),
            "a deleted allocation stays dead"
        );
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
    fn collect_moves_unreachable_records_out_of_the_swept_map() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let kept = ctx.alloc(8, 1, 0);
        let dropped = ctx.alloc(8, 1, 0);
        let mut root = kept as usize;
        ctx.root_add(&mut root as *mut usize as usize, 1);

        ctx.collect();

        assert_eq!(ctx.allocations.len(), 1, "only the live set is swept");
        assert_eq!(ctx.dead_allocations.len(), 1, "dead record is retained");
        assert!(ctx.allocations.contains_key(&(kept as usize)));
        assert!(ctx.dead_allocations.contains(&(dropped as usize)));
        assert!(ctx.is_live(kept as usize));
        assert!(!ctx.is_live(dropped as usize));
        // A second collection does not touch or discard the dead record.
        ctx.collect();
        assert_eq!(ctx.allocations.len(), 1);
        assert_eq!(ctx.dead_allocations.len(), 1);
    }

    #[test]
    #[ignore = "P24 exit-criterion timing probe; run serialized in release mode"]
    fn p24_sweep_time_is_independent_of_retained_dead_entries() {
        use std::hint::black_box;
        use std::time::{Duration, Instant};

        fn median_sweep(ctx: &mut Context) -> (Duration, f64) {
            const WARMUP_SAMPLES: usize = 3;
            const TIMED_SAMPLES: usize = 11;
            const SWEEPS_PER_SAMPLE: usize = 256;

            let mut samples = Vec::with_capacity(TIMED_SAMPLES);
            for sample in 0..WARMUP_SAMPLES + TIMED_SAMPLES {
                let mut total = Duration::ZERO;
                for _ in 0..SWEEPS_PER_SAMPLE {
                    for allocation in ctx.allocations.values_mut() {
                        allocation.marked = true;
                    }
                    let start = Instant::now();
                    ctx.sweep_dev_allocations(0);
                    total += start.elapsed();
                    black_box(ctx.allocations.len());
                }
                if sample >= WARMUP_SAMPLES {
                    samples.push(total.as_secs_f64() / SWEEPS_PER_SAMPLE as f64);
                }
            }
            samples.sort_by(f64::total_cmp);
            let median = samples[samples.len() / 2];
            let spread = ((samples[samples.len() - 1] - median) / median)
                .max((median - samples[0]) / median);
            assert!(
                spread <= 0.20,
                "P24 sweep measurement spread {:.1}% exceeds the ±20% publication gate",
                spread * 100.0
            );
            (Duration::from_secs_f64(median), spread)
        }

        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        for _ in 0..120_005 {
            assert!(!ctx.alloc(8, 1, 0).is_null());
        }
        let (low, low_spread) = median_sweep(&mut ctx);
        assert_eq!(ctx.allocation_count(), 120_005);

        for _ in 0..600_000 {
            let handle = ctx.alloc(8, 1, 0);
            assert!(!handle.is_null());
            ctx.delete(handle as usize, 0);
        }
        let (high, high_spread) = median_sweep(&mut ctx);
        assert_eq!(ctx.allocations.len(), 120_005);
        assert_eq!(ctx.dead_allocations.len(), 600_000);
        assert_eq!(ctx.retained_allocations.len(), 600_000);
        assert_eq!(ctx.allocation_count(), 720_005);

        eprintln!(
            "P24 dev sweep medians: 120,005 total = {:.3} ms (spread ±{:.1}%); \
             720,005 total = {:.3} ms (spread ±{:.1}%); 120,005 live at both points",
            low.as_secs_f64() * 1_000.0,
            low_spread * 100.0,
            high.as_secs_f64() * 1_000.0,
            high_spread * 100.0,
        );
        // A wide guard catches a cumulative-map walk without turning
        // ordinary sub-millisecond timing noise into a gate.
        assert!(
            high <= low.saturating_mul(3) / 2 + Duration::from_micros(100),
            "sweep grew with retained-dead entries: {low:?} -> {high:?}"
        );
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
        assert!(
            ctx.is_live(inner as usize),
            "element reached via data pointer"
        );
    }

    // §8.1a-1: array growth with diagnostics off frees each retired data
    // block instead of retaining it poisoned, so allocation_count does not
    // grow with the number of capacity doublings. Diagnostic mode retains
    // them.
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
        assert!(dev.set_freed_handle_diagnostics(true, 0, usize::MAX));
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
            "diagnostic mode retains retired blocks: dev {dev_count} vs ship {ship_count}"
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
        assert!(
            stats.chunks.load(SeqCst) >= 2,
            "distinct classes use distinct chunks"
        );
        assert_eq!(stats.large.load(SeqCst), 1);
        drop(ctx);
        assert_eq!(stats.chunks.load(SeqCst), 0, "drop must free every chunk");
        assert_eq!(
            stats.large.load(SeqCst),
            0,
            "drop must free every large record"
        );
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
        assert!(
            !ctx.is_live(big as usize + 8),
            "interior address is not a payload"
        );
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
        assert!(
            ctx.is_live(inner as usize),
            "traced through the large payload"
        );
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
