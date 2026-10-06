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
//! next same-class `alloc` pops it. The allocator zeroes classes that can
//! hold handles. A string writer replaces all exposed bytes, so it does
//! not zero that class. Larger allocations are individual system
//! allocations with their own record. Double delete and use-after-delete
//! are undefined here (Q6, trusted scripts). Enabling freed-handle
//! diagnostics switches either construction path to exact-size allocation
//! and budgeted retain-and-poison at or above the configured payload
//! threshold.
//!
//! # Collection
//!
//! `Context.collect()` never runs unbidden (design invariant 2). Roots are the
//! addresses generated code registers: module-global slots
//! (`root_add`) and per-call shadow frames of managed locals
//! (`shadow_push`/`shadow_pop`). Marking is conservative for each class
//! that can hold handles. Its payload words are candidates for live
//! payload addresses. This covers reference-class fields, array elements,
//! array data pointers, and coroutine frame slots without layout metadata.
//! A class that holds no handle is marked without a payload scan.
//! Conservative marking can retain garbage; it never frees a reachable
//! allocation.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::hash::{BuildHasherDefault, Hasher};

use crate::trap::{TrapKind, TrapRecord};
use crate::worker::{
    PostResult, QueueDescriptor, Worker, WorkerEntry, WorkerInit, WorkerOutcome, WorkerSet,
};

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

/// Writes the state, class, and position words of an allocation header.
///
/// # Safety
///
/// `base` must point to at least [`HEADER_SIZE`] writable bytes.
unsafe fn write_header(base: *mut u8, class_id: u32, pos_id: u32) {
    // SAFETY: the caller supplies a complete writable header.
    unsafe {
        base.cast::<u64>().write(LIVE_STATE);
        base.add((HEADER_SIZE as i32 + CLASS_ID_OFFSET) as usize)
            .cast::<u32>()
            .write(class_id);
        base.add((HEADER_SIZE as i32 + POS_ID_OFFSET) as usize)
            .cast::<u32>()
            .write(pos_id);
    }
}

/// Reads the class word of an initialized allocation header.
///
/// # Safety
///
/// `base` must point to a readable initialized allocation header.
unsafe fn header_class_id(base: *const u8) -> u32 {
    // SAFETY: the caller supplies a complete readable header.
    unsafe {
        base.add((HEADER_SIZE as i32 + CLASS_ID_OFFSET) as usize)
            .cast::<u32>()
            .read()
    }
}

/// Reads the position word of an initialized allocation header.
///
/// # Safety
///
/// `base` must point to a readable initialized allocation header.
unsafe fn header_pos_id(base: *const u8) -> u32 {
    // SAFETY: the caller supplies a complete readable header.
    unsafe {
        base.add((HEADER_SIZE as i32 + POS_ID_OFFSET) as usize)
            .cast::<u32>()
            .read()
    }
}

fn set_observer<T: Copy>(
    slot: &mut Option<T>,
    userdata_slot: &mut *mut c_void,
    observer: Option<T>,
    userdata: *mut c_void,
) {
    *slot = observer;
    *userdata_slot = if observer.is_some() {
        userdata
    } else {
        std::ptr::null_mut()
    };
}

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

/// One live array whose unused data tail contains a nonzero byte.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrayTailViolation {
    /// Address of the live array header allocation.
    pub handle: usize,
    /// Current number of live elements in the array.
    pub len: u64,
    /// Offset of the first nonzero byte in the data allocation.
    pub offset: usize,
}

#[inline]
fn class_holds_no_handle(class_id: u32) -> bool {
    matches!(class_id, CLASS_STRING)
}

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
    class_id: u32,
    marked: bool,
}

#[derive(Clone, Copy)]
enum MarkSource {
    Root {
        set: &'static str,
        index: usize,
        word: usize,
    },
    Payload {
        class_id: u32,
        address: usize,
        word: usize,
    },
}

#[derive(Clone, Copy)]
enum MarkTraceTarget {
    All,
    Strings,
    Address(usize),
}

struct MarkTracer {
    target: MarkTraceTarget,
    records: Vec<String>,
}

impl MarkTracer {
    fn new(target: MarkTraceTarget) -> Self {
        Self {
            target,
            records: Vec::new(),
        }
    }

    fn matches(&self, address: usize, class_id: u32) -> bool {
        matches!(self.target, MarkTraceTarget::All)
            || matches!(self.target, MarkTraceTarget::Strings if class_id == CLASS_STRING)
            || matches!(self.target, MarkTraceTarget::Address(target) if target == address)
    }

    fn record(&mut self, address: usize, class_id: u32, source: MarkSource) {
        if !self.matches(address, class_id) {
            return;
        }
        let source = match source {
            MarkSource::Root { set, index, word } => {
                format!("source=root set={set} index={index} word={word} value=0x{address:x}")
            }
            MarkSource::Payload {
                class_id,
                address: parent,
                word,
            } => format!(
                "source=payload class_id={class_id} address=0x{parent:x} word={word} \
                 value=0x{address:x}"
            ),
        };
        let record =
            format!("SUBSCRIPT_MARK_TRACE payload=0x{address:x} class_id={class_id} {source}");
        eprintln!("{record}");
        self.records.push(record);
    }
}

fn mark_trace_target() -> Option<MarkTraceTarget> {
    // Set SUBSCRIPT_MARK_TRACE to one payload address (decimal or 0x-prefixed
    // hexadecimal), `strings`, or `all`. The marker prints each word that
    // pushes a selected allocation. The variable is absent by default.
    let value = std::env::var("SUBSCRIPT_MARK_TRACE").ok()?;
    if value == "all" {
        return Some(MarkTraceTarget::All);
    }
    if value == "strings" {
        return Some(MarkTraceTarget::Strings);
    }
    let address = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or_else(
            || value.parse::<usize>(),
            |digits| usize::from_str_radix(digits, 16),
        );
    match address {
        Ok(address) => Some(MarkTraceTarget::Address(address)),
        Err(error) => {
            eprintln!("SUBSCRIPT_MARK_TRACE ignored invalid payload address `{value}`: {error}");
            None
        }
    }
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
pub(crate) struct AddressHasher(u64);

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

/// A registered C-callback binding. The language's function value
/// is a `(code, env)` pair with the calling convention `(ctx, env,
/// args...)`; a C callback wants a bare `(fnptr, void* userdata)`. A
/// generic C-ABI trampoline ([`crate::ffi::subscript_rt_cb_trampoline`]) bridges
/// the two: the record is what the trampoline receives through the C
/// `userdata` slot, so it carries everything the language convention needs
/// — the Context, the language `code`/`env`, and the *real* userdata the
/// script registered. Records live for the whole Context (the Q13
/// lifetime rule: userdata must outlive the registration that holds it).
///
/// The record carries **two** userdata slots (`userdata1`, `userdata2`,
/// §14.4), both delivered to the language callback. A callback-info
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
    pub(crate) trap_flag: u32,
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
    // Active frames are tracked separately while a callback is on the stack
    // so explicit collection keeps the running frame alive.
    // §94 scheduler state: runnable continuations in FIFO order, and the
    // frames that wait for the next host checkpoint.
    pub(crate) async_ready: VecDeque<AsyncJob>,
    async_parked: VecDeque<*mut u8>,
    // §94.2: clearance transfers the recorded ready job to stopped storage.
    async_trapping: Option<(*mut u8, TrapKind)>,
    async_stopped: Vec<*mut u8>,
    active_async_frames: Vec<usize>,
    // §70 held async handles. The reference count itself occupies the
    // frame header's four-byte `reserved` word; Context metadata holds
    // reload provenance, the fulfilled-value size the scheduler needs, the
    // cached completion, and the continuations registered on the frame.
    pub(crate) async_frames: HashMap<usize, AsyncFrameMeta>,
    // §113.2 rule 4: live payload bytes (§18.2d) in both memory modes.
    // Every site that changes the live set moves it, so a per-frame host
    // read costs the same at every live count.
    live_bytes_counter: usize,
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
    // compiler.md §115.6 rule 1: the Error object behind word state 2.
    pub(crate) pending_exception: Option<crate::exception::PendingException>,
    // compiler.md §115.5 rule 7: the exceptions that wait while the hooks
    // of an exception exit run, innermost last.
    pub(crate) parked_exceptions: Vec<crate::exception::PendingException>,
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
    // The live set of §111: the open and the active callback
    // registrations. `crate::registration` owns every operation on it.
    pub(crate) registrations: crate::registration::RegistrationSet,
    // Transient JSON output builders (stdlib.md §13). Untracked
    // serializers create no active-reference set; tracked ones do so
    // explicitly.
    json_builders: crate::json::JsonBuilders,
    // Transient parsed JSON syntax trees. They contain no language
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
#[path = "context/tests.rs"]
mod tests;

#[path = "context/async_scheduler.rs"]
mod async_scheduler;
pub(crate) use async_scheduler::AsyncFrameMeta;
use async_scheduler::AsyncJob;

#[path = "context/lifecycle.rs"]
mod lifecycle;

#[path = "context/memory.rs"]
mod memory;

#[cfg(test)]
#[path = "context/async_all_tests.rs"]
mod async_all_tests;
