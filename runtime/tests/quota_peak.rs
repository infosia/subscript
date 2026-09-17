//! §109.4 rule 2 total check: no buffer sized by script input exists
//! outside the allocation quota.
//!
//! A counting global allocator records the peak live bytes of the
//! process. Each covered `subscript_rt_*` entry runs under a small quota
//! with a request that would produce about 256 MiB. The four entries
//! that keep a bounded multiple of their input instead (§109.4 rule 2)
//! run on an input the quota holds, whose multiple passes the quota.
//! Each must record the `AllocationQuota` trap and leave the peak under
//! the quota plus a fixed slack.
//!
//! The second test derives the entry list from `runtime/src/ffi.rs` over
//! every `subscript_rt_` export, whatever its family. An entry that is
//! neither covered nor listed with a reason fails it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use subscript_runtime::ffi;
use subscript_runtime::{Context, TrapKind};

/// Live bytes the process holds, as the allocator sees them.
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// The largest value [`LIVE`] reached since the last reset.
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// The smallest value [`LIVE`] reached since the last reset.
///
/// The three counters are process-wide, and the test harness frees each
/// finished test's captured output and bookkeeping outside every test
/// body, so no lock a test holds keeps `LIVE` still. A measurement
/// therefore reads its window as `PEAK - FLOOR`: the largest excursion
/// above the lowest live value of the window. Reading it as
/// `PEAK - before` under-counts by whatever another thread frees between
/// the reset and the request.
static FLOOR: AtomicUsize = AtomicUsize::new(0);

/// The system allocator with a live-bytes counter and a peak record.
struct Counting;

// SAFETY: every method forwards to the system allocator with the same
// pointer and layout it was given; the counters add no requirement.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_low(LIVE.fetch_sub(layout.size(), Ordering::Relaxed) - layout.size());
        // SAFETY: forwarded caller contract.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let fresh = unsafe { System.realloc(pointer, layout, new_size) };
        if !fresh.is_null() {
            let live = LIVE.fetch_add(new_size, Ordering::Relaxed) + new_size;
            record(live);
            record_low(LIVE.fetch_sub(layout.size(), Ordering::Relaxed) - layout.size());
        }
        fresh
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size());
        }
        pointer
    }
}

/// Raises the peak to `live` when `live` is larger.
fn record(live: usize) {
    PEAK.fetch_max(live, Ordering::Relaxed);
}

/// Lowers the floor to `live` when `live` is smaller.
fn record_low(live: usize) {
    FLOOR.fetch_min(live, Ordering::Relaxed);
}

/// Opens a measurement window at the current live bytes.
fn open_window() -> usize {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.store(live, Ordering::Relaxed);
    FLOOR.store(live, Ordering::Relaxed);
    live
}

/// The bytes the window held at once: the peak above its own floor.
///
/// The second value is how far the floor fell below the value
/// [`open_window`] read. It is the bytes another thread freed inside the
/// window, and a test prints it beside its measurement.
fn close_window(opened: usize) -> (usize, usize) {
    let floor = FLOOR.load(Ordering::Relaxed);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(floor);
    (peak, opened.saturating_sub(floor))
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The counters are process-wide, so one test at a time reads them.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The quota every covered entry runs under, in bytes.
const QUOTA: u64 = 65_536;

/// The bytes a covered entry may hold above the quota.
const SLACK: usize = 1024 * 1024;

/// The result size every covered request asks for, in bytes.
const HUGE: usize = 256 * 1024 * 1024;

/// A Context with the quota set and the peak counter reset.
fn quota_context() -> Box<Context> {
    let mut ctx = Context::new();
    ctx.set_alloc_quota(QUOTA);
    ctx
}

/// What one covered request did.
struct Measurement {
    entry: &'static str,
    peak: usize,
    /// Bytes another thread freed inside the window (§ the `FLOOR` note).
    fell: usize,
    trap: Option<TrapKind>,
}

impl Measurement {
    /// The reason this request breaks §109.4 rule 2, if it does.
    fn violation(&self) -> Option<String> {
        let entry = self.entry;
        if self.trap != Some(TrapKind::AllocationQuota) {
            return Some(format!(
                "{entry}: a request of {HUGE} bytes under a {QUOTA}-byte quota \
                 recorded {:?}, not the quota trap",
                self.trap
            ));
        }
        if self.peak >= QUOTA as usize + SLACK {
            return Some(format!(
                "{entry}: peak {} bytes is over the quota {QUOTA} plus {SLACK}",
                self.peak
            ));
        }
        None
    }
}

/// Runs `request` and reports the peak bytes it added.
///
/// The peak resets after the inputs exist, so the value is the bytes the
/// entry itself holds at once. The measurement is checked at the end of
/// the test, so one run reports every entry that is over the bound.
///
/// `request` takes no Context: each case drives the entry through its own
/// `*mut Context` and reads the trap through the same pointer. A
/// `&mut Context` beside that pointer is a second path to the same
/// bytes, and an optimized build is free to keep a read of it in a
/// register across the call.
fn peak_of(entry: &'static str, ctx: &mut Context, request: impl FnOnce()) -> Measurement {
    let opened = open_window();
    request();
    let (peak, fell) = close_window(opened);
    Measurement {
        entry,
        peak,
        fell,
        trap: ctx.trap_record().map(|record| record.kind),
    }
}

/// A live string handle of `len` copies of `byte`.
fn string_of(ctx: &mut Context, byte: u8, len: usize) -> *const u8 {
    let handle = ctx.alloc_str(&vec![byte; len], 0);
    assert!(!handle.is_null(), "the input string must fit the quota");
    handle.cast_const()
}

/// A descending comparator of the shape `sort` calls: `(ctx, env, a, b)
/// -> i32`.
unsafe extern "C" fn descending(_ctx: *mut Context, _env: *const u8, a: f64, b: f64) -> i32 {
    if a > b {
        -1
    } else if a < b {
        1
    } else {
        0
    }
}

/// A live string handle of `text`.
fn string(ctx: &mut Context, text: &[u8]) -> *const u8 {
    let handle = ctx.alloc_str(text, 0);
    assert!(!handle.is_null(), "the input string must fit the quota");
    handle.cast_const()
}

/// The entries a script can drive to about 256 MiB. Each one has a case
/// in [`every_covered_entry_stays_under_the_quota`].
const COVERED: &[&str] = &[
    "subscript_rt_str_repeat",
    "subscript_rt_str_pad_start",
    "subscript_rt_str_pad_end",
    "subscript_rt_str_to_upper",
    "subscript_rt_str_to_lower",
    "subscript_rt_str_replace",
    "subscript_rt_str_replace_all",
    "subscript_rt_regex_replace",
    "subscript_rt_regex_replace_all",
    "subscript_rt_arr_join",
    "subscript_rt_json_raw",
    "subscript_rt_json_str",
    "subscript_rt_json_parse_begin",
    "subscript_rt_arr_sort",
    "subscript_rt_boundary_scratch_alloc",
    "subscript_rt_print",
    "subscript_rt_cb_bind",
];

/// Every other `subscript_rt_` export, with the reason a script cannot
/// size its result or its temporary.
const EXEMPT: &[(&str, &str)] = &[
    // Strings.
    (
        "subscript_rt_str_lit",
        "the result is the module's own literal, not script input",
    ),
    (
        "subscript_rt_str_from_view",
        "the result size is the host's view length",
    ),
    ("subscript_rt_str_len", "the result is one scalar"),
    (
        "subscript_rt_str_iter_code_point",
        "the result is one code point",
    ),
    (
        "subscript_rt_str_concat",
        "the result size is known first and goes into the Context allocation",
    ),
    (
        "subscript_rt_str_slice",
        "the result is a range of the receiver, which the quota holds",
    ),
    ("subscript_rt_str_eq", "the result is one scalar"),
    ("subscript_rt_str_index_of", "the result is one scalar"),
    ("subscript_rt_str_last_index_of", "the result is one scalar"),
    ("subscript_rt_str_includes", "the result is one scalar"),
    ("subscript_rt_str_starts_with", "the result is one scalar"),
    ("subscript_rt_str_ends_with", "the result is one scalar"),
    ("subscript_rt_str_char_code_at", "the result is one scalar"),
    (
        "subscript_rt_str_substring",
        "the result is a range of the receiver, which the quota holds",
    ),
    (
        "subscript_rt_str_substr",
        "the result is a range of the receiver, which the quota holds",
    ),
    ("subscript_rt_str_char_at", "the result is one code point"),
    ("subscript_rt_str_code_point_at", "the result is one scalar"),
    (
        "subscript_rt_str_split",
        "every piece is a Context allocation that the quota checks",
    ),
    (
        "subscript_rt_str_trim",
        "the result is a sub-slice of the receiver, which the quota holds",
    ),
    (
        "subscript_rt_str_trim_start",
        "the result is a sub-slice of the receiver, which the quota holds",
    ),
    (
        "subscript_rt_str_trim_end",
        "the result is a sub-slice of the receiver, which the quota holds",
    ),
    (
        "subscript_rt_str_data",
        "the result is a pointer into an allocation that already exists",
    ),
    // Regular expressions.
    (
        "subscript_rt_regex_new",
        "the result size is the pattern and the flags, which the quota holds",
    ),
    ("subscript_rt_regex_test", "the result is one scalar"),
    (
        "subscript_rt_regex_source",
        "the result is a handle the Context already holds",
    ),
    (
        "subscript_rt_regex_flags",
        "the result is a handle the Context already holds",
    ),
    ("subscript_rt_regex_search", "the result is one scalar"),
    (
        "subscript_rt_regex_split",
        "every piece is a Context allocation that the quota checks",
    ),
    ("subscript_rt_regex_match_start", "the result is one scalar"),
    ("subscript_rt_regex_match_end", "the result is one scalar"),
    // JSON output.
    ("subscript_rt_json_begin", "the result is one builder id"),
    (
        "subscript_rt_json_begin_tracked",
        "the result is one builder id",
    ),
    (
        "subscript_rt_json_finish",
        "the builder holds at most the quota headroom",
    ),
    (
        "subscript_rt_json_f32",
        "the append is one formatted scalar",
    ),
    (
        "subscript_rt_json_f64",
        "the append is one formatted scalar",
    ),
    (
        "subscript_rt_json_bool",
        "the append is one formatted scalar",
    ),
    (
        "subscript_rt_json_date",
        "the append is one ISO timestamp of fixed length",
    ),
    ("subscript_rt_json_null", "the append is four bytes"),
    ("subscript_rt_json_visit", "the result is one scalar"),
    ("subscript_rt_json_leave", "the call records no bytes"),
    // JSON input.
    (
        "subscript_rt_json_parse_end",
        "the call releases a document",
    ),
    (
        "subscript_rt_json_parse_root",
        "the result is one node handle",
    ),
    (
        "subscript_rt_json_parse_is_kind",
        "the result is one scalar",
    ),
    (
        "subscript_rt_json_parse_number_fits",
        "the result is one scalar",
    ),
    ("subscript_rt_json_parse_number", "the result is one scalar"),
    (
        "subscript_rt_json_parse_integer",
        "the result is one scalar",
    ),
    ("subscript_rt_json_parse_bool", "the result is one scalar"),
    (
        "subscript_rt_json_parse_string",
        "the result is one parsed string of the document the quota holds",
    ),
    (
        "subscript_rt_json_parse_array_len",
        "the result is one scalar",
    ),
    (
        "subscript_rt_json_parse_array_get",
        "the result is one node handle",
    ),
    (
        "subscript_rt_json_parse_object_get",
        "the result is one node handle",
    ),
    // Arrays.
    ("subscript_rt_arr_index_of", "the result is one scalar"),
    ("subscript_rt_arr_last_index_of", "the result is one scalar"),
    ("subscript_rt_arr_includes", "the result is one scalar"),
    (
        "subscript_rt_arr_slice",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_arr_fill",
        "the call writes the receiver in place",
    ),
    (
        "subscript_rt_arr_reverse",
        "the call writes the receiver in place",
    ),
    (
        "subscript_rt_arr_concat",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_arr_splice",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_arr_shift",
        "the call writes the receiver in place",
    ),
    (
        "subscript_rt_arr_unshift",
        "the result is one scalar and the receiver grows through the Context path",
    ),
    (
        "subscript_rt_arr_copy_within",
        "the call writes the receiver in place",
    ),
    ("subscript_rt_arr_for_each", "the call records no result"),
    (
        "subscript_rt_arr_map",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_arr_filter",
        "the result array grows through the Context path, which the quota checks",
    ),
    ("subscript_rt_arr_reduce", "the result is one element"),
    ("subscript_rt_arr_reduce_right", "the result is one element"),
    ("subscript_rt_arr_some", "the result is one scalar"),
    ("subscript_rt_arr_every", "the result is one scalar"),
    ("subscript_rt_arr_find_index", "the result is one scalar"),
    // Maps and sets.
    (
        "subscript_rt_assoc_iter_begin",
        "the result is one iteration handle",
    ),
    (
        "subscript_rt_assoc_iter_copy",
        "the result is one entry of the receiver",
    ),
    ("subscript_rt_assoc_iter_end", "the call records no result"),
    ("subscript_rt_assoc_size", "the result is one scalar"),
    ("subscript_rt_assoc_has", "the result is one scalar"),
    ("subscript_rt_assoc_delete", "the result is one scalar"),
    (
        "subscript_rt_assoc_clear",
        "the call writes the receiver in place",
    ),
    // Workers.
    (
        "subscript_rt_worker_spawn",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_post",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_poll",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_close",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_join",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_inbox_wait",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_inbox_poll",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    (
        "subscript_rt_worker_outbox_post",
        "§109.2 S025 rejects the Worker surface under the profile",
    ),
    // Every other family. The total check reads every export of
    // `ffi.rs`, so each one is covered by a case or listed here.
    (
        "subscript_rt_alloc",
        "the result size is known first and goes into the Context allocation",
    ),
    (
        "subscript_rt_array_byte_range",
        "the result is a range of the receiver, which the quota holds",
    ),
    (
        "subscript_rt_array_data",
        "the result is a pointer into an allocation that already exists",
    ),
    (
        "subscript_rt_array_from_bytes",
        "the result size is the readable span the caller gives",
    ),
    ("subscript_rt_array_len", "the result is one scalar"),
    ("subscript_rt_array_new", "the result is one empty array"),
    ("subscript_rt_array_pop", "the result is one element copy"),
    (
        "subscript_rt_array_ptr",
        "the result is a pointer into an allocation that already exists",
    ),
    (
        "subscript_rt_array_push",
        "the receiver grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_array_spread_array",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_array_spread_assoc",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_array_spread_fixed",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_array_spread_string",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_array_with_capacity",
        "the capacity goes into the Context allocation, which the quota checks",
    ),
    (
        "subscript_rt_async_await",
        "the call records one continuation",
    ),
    (
        "subscript_rt_async_complete",
        "the result is one element copy",
    ),
    ("subscript_rt_async_is_stale", "the result is one scalar"),
    (
        "subscript_rt_async_kick",
        "the call runs a frame the quota already holds",
    ),
    (
        "subscript_rt_async_missing_completion",
        "the call records one trap",
    ),
    (
        "subscript_rt_async_park",
        "the call moves one frame between queues",
    ),
    ("subscript_rt_async_register", "the call records one frame"),
    (
        "subscript_rt_async_release",
        "the call decrements one count",
    ),
    (
        "subscript_rt_async_release_array",
        "the call walks an array the quota holds",
    ),
    (
        "subscript_rt_async_result",
        "the result is one element copy",
    ),
    ("subscript_rt_async_retain", "the call increments one count"),
    (
        "subscript_rt_async_retain_array",
        "the call walks an array the quota holds",
    ),
    (
        "subscript_rt_boundary_scratch_mark",
        "the result is one scalar",
    ),
    (
        "subscript_rt_boundary_scratch_release",
        "the call releases the scratch scope",
    ),
    (
        "subscript_rt_cb_trampoline",
        "the call forwards one message to a script callback",
    ),
    (
        "subscript_rt_collect",
        "the call releases memory and allocates none",
    ),
    ("subscript_rt_ctx_async_pending", "the result is one scalar"),
    (
        "subscript_rt_ctx_async_step",
        "the call drains queues the quota already holds",
    ),
    (
        "subscript_rt_ctx_async_unfinished",
        "the result is one scalar",
    ),
    ("subscript_rt_ctx_charged_bytes", "the result is one scalar"),
    (
        "subscript_rt_ctx_clear_trap",
        "the call clears the trap record",
    ),
    (
        "subscript_rt_ctx_collect",
        "the call releases memory and allocates none",
    ),
    (
        "subscript_rt_ctx_enter_script",
        "the call records one depth",
    ),
    ("subscript_rt_ctx_exit_script", "the call records one depth"),
    (
        "subscript_rt_ctx_fail_alloc_after",
        "the call stores one host counter",
    ),
    (
        "subscript_rt_ctx_interrupt_handle",
        "the result is one interrupt cell of fixed size",
    ),
    (
        "subscript_rt_ctx_live_allocations",
        "the result is one scalar",
    ),
    ("subscript_rt_ctx_live_bytes", "the result is one scalar"),
    (
        "subscript_rt_ctx_new",
        "the result is one Context the host owns",
    ),
    ("subscript_rt_ctx_release", "the call frees the Context"),
    (
        "subscript_rt_ctx_reserved_bytes",
        "the result is one scalar",
    ),
    (
        "subscript_rt_ctx_seed_random",
        "the call reseeds a fixed-size generator state",
    ),
    (
        "subscript_rt_ctx_set_alloc_quota",
        "the call stores the quota itself",
    ),
    (
        "subscript_rt_ctx_set_binding_count_advisory",
        "the call stores one host threshold",
    ),
    (
        "subscript_rt_ctx_set_diagnostics_observer",
        "the call stores one host callback",
    ),
    (
        "subscript_rt_ctx_set_freed_handle_diagnostics",
        "the call stores one host flag",
    ),
    (
        "subscript_rt_ctx_set_now",
        "the call stores one host timestamp",
    ),
    (
        "subscript_rt_ctx_set_print_observer",
        "the call stores one host callback",
    ),
    (
        "subscript_rt_ctx_set_regex_budget",
        "the call stores one host budget",
    ),
    (
        "subscript_rt_ctx_set_stack_budget",
        "the call stores one host budget",
    ),
    (
        "subscript_rt_ctx_set_trap_observer",
        "the call stores one host callback",
    ),
    (
        "subscript_rt_ctx_stdout",
        "the result borrows the capture sink, which is the host's buffer",
    ),
    ("subscript_rt_ctx_trap_kind", "the result is one scalar"),
    (
        "subscript_rt_ctx_trap_message",
        "the result borrows the trap message the runtime already holds",
    ),
    ("subscript_rt_ctx_trap_pos_id", "the result is one scalar"),
    (
        "subscript_rt_ctx_visit_live_allocations",
        "the call visits the live set and allocates nothing",
    ),
    ("subscript_rt_date_get", "the result is one scalar"),
    ("subscript_rt_date_new", "the result is one scalar"),
    ("subscript_rt_date_now", "the result is one scalar"),
    (
        "subscript_rt_date_to_iso",
        "the result is one ISO timestamp of fixed length",
    ),
    ("subscript_rt_date_utc", "the result is one scalar"),
    ("subscript_rt_delete", "the call releases one allocation"),
    ("subscript_rt_f16_from_f64", "the result is one scalar"),
    ("subscript_rt_f16_to_f64", "the result is one scalar"),
    ("subscript_rt_fixed_arr_every", "the result is one scalar"),
    (
        "subscript_rt_fixed_arr_filter",
        "the result array grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_fixed_arr_find_index",
        "the result is one scalar",
    ),
    (
        "subscript_rt_fixed_arr_for_each",
        "the call records no result",
    ),
    (
        "subscript_rt_fixed_arr_map",
        "the result array grows through the Context path, which the quota checks",
    ),
    ("subscript_rt_fixed_arr_reduce", "the result is one element"),
    (
        "subscript_rt_fixed_arr_reduce_right",
        "the result is one element",
    ),
    ("subscript_rt_fixed_arr_some", "the result is one scalar"),
    ("subscript_rt_fmod", "the result is one scalar"),
    (
        "subscript_rt_fmt_bool",
        "the result is one formatted scalar",
    ),
    ("subscript_rt_fmt_f32", "the result is one formatted scalar"),
    ("subscript_rt_fmt_f64", "the result is one formatted scalar"),
    (
        "subscript_rt_globals_init",
        "the result size is the module image's global block, not script input",
    ),
    ("subscript_rt_interrupt_set", "the call stores one flag"),
    ("subscript_rt_map_for_each", "the call records no result"),
    ("subscript_rt_map_get", "the result is one value copy"),
    ("subscript_rt_map_get_or", "the result is one value copy"),
    (
        "subscript_rt_map_group_by",
        "every group is a Context allocation that the quota checks",
    ),
    ("subscript_rt_map_new", "the result is one empty map"),
    (
        "subscript_rt_map_set",
        "the entry grows through the Context path, which the quota checks",
    ),
    ("subscript_rt_math_clz32", "the result is one scalar"),
    (
        "subscript_rt_math_f32_from_bits",
        "the result is one scalar",
    ),
    ("subscript_rt_math_f32_to_bits", "the result is one scalar"),
    ("subscript_rt_math_fround", "the result is one scalar"),
    ("subscript_rt_math_imul", "the result is one scalar"),
    ("subscript_rt_math_random", "the result is one scalar"),
    ("subscript_rt_num_is_finite", "the result is one scalar"),
    ("subscript_rt_num_is_integer", "the result is one scalar"),
    ("subscript_rt_num_is_nan", "the result is one scalar"),
    (
        "subscript_rt_num_is_safe_integer",
        "the result is one scalar",
    ),
    ("subscript_rt_num_parse_float", "the result is one scalar"),
    ("subscript_rt_num_parse_int", "the result is one scalar"),
    (
        "subscript_rt_num_to_exponential",
        "the result is one formatted scalar of at most 100 digits",
    ),
    (
        "subscript_rt_num_to_fixed",
        "the result is one formatted scalar of at most 101 digits",
    ),
    (
        "subscript_rt_num_to_precision",
        "the result is one formatted scalar of at most 100 digits",
    ),
    (
        "subscript_rt_num_to_string_f32",
        "the result is one formatted scalar bounded by the radix",
    ),
    (
        "subscript_rt_num_to_string_f64",
        "the result is one formatted scalar bounded by the radix",
    ),
    (
        "subscript_rt_root_add",
        "the call records one root range of the module image",
    ),
    ("subscript_rt_sandbox_enter", "the call reads two counters"),
    ("subscript_rt_sandbox_poll", "the call reads one flag"),
    (
        "subscript_rt_set_add",
        "the entry grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_difference",
        "the result set grows through the Context path, which the quota checks",
    ),
    ("subscript_rt_set_for_each", "the call records no result"),
    (
        "subscript_rt_set_from_array",
        "the result set grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_from_assoc",
        "the result set grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_from_fixed",
        "the result set grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_from_string",
        "the result set grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_intersection",
        "the result set grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_is_disjoint_from",
        "the result is one scalar",
    ),
    ("subscript_rt_set_is_subset_of", "the result is one scalar"),
    (
        "subscript_rt_set_is_superset_of",
        "the result is one scalar",
    ),
    ("subscript_rt_set_new", "the result is one empty set"),
    (
        "subscript_rt_set_symmetric_difference",
        "the result set grows through the Context path, which the quota checks",
    ),
    (
        "subscript_rt_set_union",
        "the result set grows through the Context path, which the quota checks",
    ),
    ("subscript_rt_shadow_pop", "the call records no bytes"),
    (
        "subscript_rt_shadow_push",
        "the call records one frame range the caller owns",
    ),
    ("subscript_rt_trap", "the call records one trap"),
    (
        "subscript_rt_trap_index_out_of_bounds",
        "the call records one trap",
    ),
    ("subscript_rt_trap_wire_enum", "the call records one trap"),
];

/// Takes the process-wide counter lock, ignoring an earlier panic.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn every_covered_entry_stays_under_the_quota() {
    let _guard = one_at_a_time();
    let mut measured: Vec<Measurement> = Vec::new();

    // `repeat(n)`: one byte, 268,435,456 copies.
    {
        let mut ctx = quota_context();
        let receiver = string(&mut ctx, b"x");
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_str_repeat", &mut ctx, || {
            // SAFETY: live exclusive Context and live string handle.
            unsafe { ffi::subscript_rt_str_repeat(pointer, receiver, HUGE as i32, 1) };
        });
        measured.push(peak);
    }

    // `padStart`/`padEnd`: a target length of 268,435,456 bytes.
    for (entry, call) in [
        (
            "subscript_rt_str_pad_start",
            ffi::subscript_rt_str_pad_start
                as unsafe extern "C" fn(*mut Context, *const u8, i32, *const u8, u32) -> *mut u8,
        ),
        ("subscript_rt_str_pad_end", ffi::subscript_rt_str_pad_end),
    ] {
        let mut ctx = quota_context();
        let receiver = string(&mut ctx, b"x");
        let pad = string(&mut ctx, b" ");
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of(entry, &mut ctx, || {
            // SAFETY: live exclusive Context and live string handles.
            unsafe { call(pointer, receiver, HUGE as i32, pad, 1) };
        });
        measured.push(peak);
    }

    // `toUpperCase`/`toLowerCase`: §109.4 rule 2 keeps a bounded
    // multiple for these two. A receiver of half the quota fits, and
    // three bytes for each receiver byte passes it.
    for (entry, call) in [
        (
            "subscript_rt_str_to_upper",
            ffi::subscript_rt_str_to_upper
                as unsafe extern "C" fn(*mut Context, *const u8, u32) -> *mut u8,
        ),
        ("subscript_rt_str_to_lower", ffi::subscript_rt_str_to_lower),
    ] {
        let mut ctx = quota_context();
        let receiver = string_of(&mut ctx, b'a', 32_768);
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of(entry, &mut ctx, || {
            // SAFETY: live exclusive Context and live string handle.
            unsafe { call(pointer, receiver, 1) };
        });
        measured.push(peak);
    }

    // `replace("", repl)`: the replacement is 16,384 `$'` tokens and the
    // receiver is 16,384 bytes, so one match writes 256 MiB.
    {
        let mut ctx = quota_context();
        let receiver = string_of(&mut ctx, b'a', 16_384);
        let pattern = string(&mut ctx, b"");
        let replacement = string(&mut ctx, &b"$'".repeat(16_384));
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_str_replace", &mut ctx, || {
            // SAFETY: live exclusive Context and live string handles.
            unsafe {
                ffi::subscript_rt_str_replace(pointer, receiver, pattern, replacement, 1);
            }
        });
        measured.push(peak);
    }

    // `replaceAll("a", repl)`: 16,384 matches of a 16,384-byte
    // replacement is 256 MiB.
    {
        let mut ctx = quota_context();
        let receiver = string_of(&mut ctx, b'a', 16_384);
        let pattern = string(&mut ctx, b"a");
        let replacement = string_of(&mut ctx, b'b', 16_384);
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_str_replace_all", &mut ctx, || {
            // SAFETY: live exclusive Context and live string handles.
            unsafe {
                ffi::subscript_rt_str_replace_all(pointer, receiver, pattern, replacement, 1);
            }
        });
        measured.push(peak);
    }

    // `replace(/a/, repl)`: one match, 16,384 `$'` tokens against a
    // 16,385-byte subject.
    {
        let mut ctx = quota_context();
        let mut subject_bytes = vec![b'a'];
        subject_bytes.extend(std::iter::repeat_n(b'b', 16_384));
        let subject = string(&mut ctx, &subject_bytes);
        let pattern = string(&mut ctx, b"a");
        let flags = string(&mut ctx, b"");
        let replacement = string(&mut ctx, &b"$'".repeat(16_384));
        let pointer: *mut Context = &mut *ctx;
        // SAFETY: live exclusive Context and live string handles.
        let regex = unsafe { ffi::subscript_rt_regex_new(pointer, pattern, flags, 1) };
        assert!(!regex.is_null(), "the RegExp must fit the quota");
        let peak = peak_of("subscript_rt_regex_replace", &mut ctx, || {
            // SAFETY: live exclusive Context and live handles.
            unsafe {
                ffi::subscript_rt_regex_replace(pointer, subject, regex, replacement, 1);
            }
        });
        measured.push(peak);
    }

    // `replaceAll(/a/g, repl)`: 16,384 matches of a 16,384-byte
    // replacement is 256 MiB.
    {
        let mut ctx = quota_context();
        let subject = string_of(&mut ctx, b'a', 16_384);
        let pattern = string(&mut ctx, b"a");
        let flags = string(&mut ctx, b"g");
        let replacement = string_of(&mut ctx, b'b', 16_384);
        let pointer: *mut Context = &mut *ctx;
        // SAFETY: live exclusive Context and live string handles.
        let regex = unsafe { ffi::subscript_rt_regex_new(pointer, pattern, flags, 1) };
        assert!(!regex.is_null(), "the RegExp must fit the quota");
        let peak = peak_of("subscript_rt_regex_replace_all", &mut ctx, || {
            // SAFETY: live exclusive Context and live handles.
            unsafe {
                ffi::subscript_rt_regex_replace_all(pointer, subject, regex, replacement, 1);
            }
        });
        measured.push(peak);
    }

    // `join(sep)`: 16,384 one-byte elements with a 16,384-byte
    // separator writes about 256 MiB.
    {
        let mut ctx = quota_context();
        let elements = 16_384usize;
        let array = ctx.array_with_capacity(elements, 1, 0);
        assert!(!array.is_null(), "the array must fit the quota");
        for _ in 0..elements {
            let element = 7u8;
            // SAFETY: a live one-byte-element array of this Context.
            assert!(
                unsafe { ctx.array_push(array, (&element as *const u8).cast(), 0) } >= 0,
                "the array must fit the quota"
            );
        }
        let separator = string_of(&mut ctx, b's', 16_384);
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_arr_join", &mut ctx, || {
            // SAFETY: live exclusive Context, live array, live separator.
            // Kind 9 is `u8` (`arrops::FmtKind::from_u32`).
            unsafe { ffi::subscript_rt_arr_join(pointer, array, separator, 9, 1) };
        });
        measured.push(peak);
    }

    // The JSON builder: 16,384 appends of a 16,384-byte piece is
    // 256 MiB. The trap stops the loop, as an emitted trap check does.
    for (entry, quoted) in [
        ("subscript_rt_json_raw", false),
        ("subscript_rt_json_str", true),
    ] {
        let mut ctx = quota_context();
        let piece = string_of(&mut ctx, b'j', 16_384);
        let pointer: *mut Context = &mut *ctx;
        // SAFETY: live exclusive Context.
        let builder = unsafe { ffi::subscript_rt_json_begin(pointer, 1) };
        assert_ne!(builder, 0, "the builder id must be live");
        let peak = peak_of(entry, &mut ctx, || {
            for _ in 0..16_384 {
                // SAFETY: live exclusive Context, live builder, live
                // string handle.
                let trapped = unsafe {
                    if quoted {
                        ffi::subscript_rt_json_str(pointer, builder, piece, 1);
                    } else {
                        ffi::subscript_rt_json_raw(pointer, builder, piece, 1);
                    }
                    (*pointer).trapped()
                };
                if trapped {
                    break;
                }
            }
        });
        measured.push(peak);
    }

    // `JSON.parse`: §109.4 rule 2 keeps a bounded multiple here too. A
    // 64,001-byte `[1,1,…]` input fits the quota, and its transient
    // document of about 40 bytes for each two input bytes passes it.
    {
        let mut ctx = quota_context();
        let mut text = vec![b'['];
        for number in 0..32_000 {
            if number > 0 {
                text.push(b',');
            }
            text.push(b'1');
        }
        text.push(b']');
        let input = string(&mut ctx, &text);
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_json_parse_begin", &mut ctx, || {
            // SAFETY: live exclusive Context and live string handle.
            unsafe { ffi::subscript_rt_json_parse_begin(pointer, input, 1) };
        });
        measured.push(peak);
    }

    // `sort(cmp)`: §109.4 rule 2 keeps a bounded multiple here too. The
    // sort holds two copies of the receiver, so a receiver of half the
    // quota passes it.
    {
        let mut ctx = quota_context();
        let elements = 4_096;
        let array = ctx.array_with_capacity(elements, 8, 0);
        assert!(!array.is_null(), "the receiver must fit the quota");
        for index in 0..elements {
            let value = (elements - index) as f64;
            // SAFETY: a live eight-byte-element array of this Context.
            assert!(
                unsafe { ctx.array_push(array, (&value as *const f64).cast(), 0) } >= 0,
                "the receiver must fit the quota"
            );
        }
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_arr_sort", &mut ctx, || {
            // SAFETY: live exclusive Context, live array, live
            // comparator. Kind 2 is `f64` (`arrops::ElemKind::from_u32`).
            unsafe {
                ffi::subscript_rt_arr_sort(
                    pointer,
                    array,
                    descending as *const u8,
                    std::ptr::null(),
                    2,
                );
            }
        });
        measured.push(peak);
    }

    // The boundary scratch block: the marshal sizes it from the length
    // of the array it lowers, and the block lives outside the Context.
    {
        let mut ctx = quota_context();
        let pointer: *mut Context = &mut *ctx;
        // SAFETY: live exclusive Context.
        let mark = unsafe { ffi::subscript_rt_boundary_scratch_mark(pointer) };
        let peak = peak_of("subscript_rt_boundary_scratch_alloc", &mut ctx, || {
            // SAFETY: live exclusive Context.
            unsafe { ffi::subscript_rt_boundary_scratch_alloc(pointer, HUGE as u64, 1) };
        });
        // SAFETY: live exclusive Context and a mark of this Context.
        unsafe { ffi::subscript_rt_boundary_scratch_release(pointer, mark) };
        measured.push(peak);
    }

    // `print`: with no observer installed the stdout sink is Context
    // memory that script output sizes. 65,536 lines of 4,096 bytes is
    // 256 MiB. The trap stops the loop, as an emitted trap check does.
    {
        let mut ctx = quota_context();
        let line = string_of(&mut ctx, b'p', 4_096);
        let pointer: *mut Context = &mut *ctx;
        let peak = peak_of("subscript_rt_print", &mut ctx, || {
            for _ in 0..(HUGE / 4_096) {
                // SAFETY: live exclusive Context and live string handle.
                let trapped = unsafe {
                    ffi::subscript_rt_print(pointer, line);
                    (*pointer).trapped()
                };
                if trapped {
                    break;
                }
            }
        });
        measured.push(peak);
    }

    // `cb_bind`: one record for each identity the script registers.
    // 6,710,886 records of 40 bytes is 256 MiB.
    {
        let mut ctx = quota_context();
        let pointer: *mut Context = &mut *ctx;
        let record = std::mem::size_of::<subscript_runtime::CallbackBinding>();
        let peak = peak_of("subscript_rt_cb_bind", &mut ctx, || {
            for index in 1..=(HUGE / record) {
                // SAFETY: live exclusive Context; `env` is null, as a
                // boundary callback's is, and each `userdata1` is a
                // distinct identity.
                let trapped = unsafe {
                    ffi::subscript_rt_cb_bind(
                        pointer,
                        descending as *const u8,
                        std::ptr::null(),
                        index as *mut u8,
                        std::ptr::null_mut(),
                    );
                    (*pointer).trapped()
                };
                if trapped {
                    break;
                }
            }
        });
        measured.push(peak);
    }

    let names: Vec<&str> = measured.iter().map(|one| one.entry).collect();
    assert_eq!(names, COVERED, "every covered entry needs one case here");
    for one in &measured {
        println!(
            "{}: peak {} bytes under a {QUOTA}-byte quota, trap {:?}, floor fell {} bytes",
            one.entry, one.peak, one.trap, one.fell
        );
    }
    let violations: Vec<String> = measured.iter().filter_map(Measurement::violation).collect();
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

/// The firing control for [`every_covered_entry_stays_under_the_quota`]:
/// with no quota the same request holds the whole result, so the bound
/// the test asserts is one the request can break.
#[test]
fn a_request_with_no_quota_holds_the_whole_result() {
    let _guard = one_at_a_time();
    let mut ctx = Context::new();
    let receiver = string(&mut ctx, b"x");
    let pointer: *mut Context = &mut *ctx;
    let megabyte = 1024 * 1024;
    let opened = open_window();
    // SAFETY: live exclusive Context and live string handle.
    let result = unsafe { ffi::subscript_rt_str_repeat(pointer, receiver, megabyte, 1) };
    let (peak, fell) = close_window(opened);
    assert!(!result.is_null(), "no quota accepts the request");
    println!("no quota: peak {peak} bytes, floor fell {fell} bytes");
    assert!(
        peak >= megabyte as usize,
        "the request must hold its result: {peak} bytes, floor fell {fell} bytes"
    );
}

/// The control of the `print` case: the sink charges only what it
/// retains (§109.4 rule 2).
///
/// With an observer installed the line is delivered and not retained, so
/// the same volume that traps above runs clean and charges nothing.
/// `take_stdout` then releases the charge the unobserved sink holds.
#[test]
fn an_observed_print_charges_nothing_and_take_stdout_releases_the_charge() {
    let _guard = one_at_a_time();

    /// Counts the lines the observer receives.
    unsafe extern "C" fn count(userdata: *mut std::ffi::c_void, _line: *const u8, _len: u64) {
        // SAFETY: the test passes a live `u64`.
        unsafe { *userdata.cast::<u64>() += 1 };
    }

    let mut ctx = quota_context();
    let line = string_of(&mut ctx, b'p', 4_096);
    let charged_before = ctx.charged_bytes();
    let pointer: *mut Context = &mut *ctx;
    let mut seen: u64 = 0;
    // SAFETY: live exclusive Context; the observer and its userdata
    // outlive the calls below.
    unsafe {
        ffi::subscript_rt_ctx_set_print_observer(
            pointer,
            Some(count),
            std::ptr::from_mut(&mut seen).cast(),
        );
    }
    let lines = HUGE / 4_096;
    for _ in 0..lines {
        // SAFETY: live exclusive Context and live string handle.
        unsafe { ffi::subscript_rt_print(pointer, line) };
    }
    assert_eq!(seen, lines as u64, "the observer must see every line");
    assert_eq!(ctx.trap_record().map(|record| record.kind), None);
    assert_eq!(
        ctx.charged_bytes(),
        charged_before,
        "an observed line charges nothing"
    );
    assert!(
        ctx.stdout_bytes().is_empty(),
        "an observed line is not retained"
    );

    // With the observer cleared the sink retains and charges, and
    // `take_stdout` releases that charge.
    // SAFETY: live exclusive Context.
    unsafe { ffi::subscript_rt_ctx_set_print_observer(pointer, None, std::ptr::null_mut()) };
    // SAFETY: live exclusive Context and live string handle.
    unsafe { ffi::subscript_rt_print(pointer, line) };
    assert_eq!(
        ctx.charged_bytes(),
        charged_before + 4_096,
        "an unobserved line charges its bytes"
    );
    assert_eq!(ctx.take_stdout().len(), 4_097);
    assert_eq!(
        ctx.charged_bytes(),
        charged_before,
        "`take_stdout` releases the sink's charge"
    );
}

#[test]
fn every_exported_entry_is_covered_or_listed_with_a_reason() {
    let _guard = one_at_a_time();
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ffi.rs"))
        .expect("read runtime/src/ffi.rs");
    let mut exported: Vec<&str> = Vec::new();
    for line in source.lines() {
        // §109.4 rule 2: every export, whatever its family.
        let Some(rest) = line
            .strip_prefix("pub unsafe extern \"C\" fn ")
            .or_else(|| line.strip_prefix("pub extern \"C\" fn "))
        else {
            continue;
        };
        let name = rest.split('(').next().unwrap_or_default();
        if name.starts_with("subscript_rt_") {
            exported.push(name);
        }
    }
    assert!(
        exported.len() > 200,
        "the reader found {} entries; it is wrong",
        exported.len()
    );

    let mut unlisted: Vec<&str> = Vec::new();
    for name in &exported {
        let covered = COVERED.contains(name);
        let exempt = EXEMPT.iter().any(|(entry, _)| entry == name);
        assert!(
            !(covered && exempt),
            "{name} is both covered and exempt; one list decides"
        );
        if !covered && !exempt {
            unlisted.push(name);
        }
    }
    assert!(
        unlisted.is_empty(),
        "each entry is covered by a peak case or listed with a reason: {unlisted:?}"
    );

    for (entry, reason) in EXEMPT {
        assert!(
            exported.contains(entry),
            "{entry} is listed but `ffi.rs` exports no such entry"
        );
        assert!(!reason.is_empty(), "{entry} needs a reason");
    }
    for entry in COVERED {
        assert!(
            exported.contains(entry),
            "{entry} is covered but `ffi.rs` exports no such entry"
        );
    }
    assert_eq!(
        COVERED.len() + EXEMPT.len(),
        exported.len(),
        "the two lists must name every entry exactly once"
    );
}
