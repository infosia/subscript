//! §109.4 rule 2 total check: no buffer sized by script input exists
//! outside the allocation quota.
//!
//! A counting global allocator records the peak live bytes of the
//! process. Each covered `subscript_rt_*` entry runs under a small quota
//! with a request that would produce about 256 MiB. Each must record the
//! `AllocationQuota` trap and leave the peak under the quota plus a
//! fixed slack.
//!
//! The second test derives the entry list from `runtime/src/ffi.rs`. An
//! entry that is neither covered nor listed with a reason fails it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use subscript_runtime::ffi;
use subscript_runtime::{Context, TrapKind};

/// Live bytes the process holds, as the allocator sees them.
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// The largest value [`LIVE`] reached since the last reset.
static PEAK: AtomicUsize = AtomicUsize::new(0);

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
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: forwarded caller contract.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let fresh = unsafe { System.realloc(pointer, layout, new_size) };
        if !fresh.is_null() {
            let live = LIVE.fetch_add(new_size, Ordering::Relaxed) + new_size;
            record(live);
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
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
fn peak_of(
    entry: &'static str,
    ctx: &mut Context,
    request: impl FnOnce(&mut Context),
) -> Measurement {
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    request(ctx);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(before);
    Measurement {
        entry,
        peak,
        trap: ctx.trap_record().map(|record| record.kind),
    }
}

/// A live string handle of `len` copies of `byte`.
fn string_of(ctx: &mut Context, byte: u8, len: usize) -> *const u8 {
    let handle = ctx.alloc_str(&vec![byte; len], 0);
    assert!(!handle.is_null(), "the input string must fit the quota");
    handle.cast_const()
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
    "subscript_rt_str_replace",
    "subscript_rt_str_replace_all",
    "subscript_rt_regex_replace",
    "subscript_rt_regex_replace_all",
    "subscript_rt_arr_join",
    "subscript_rt_json_raw",
    "subscript_rt_json_str",
];

/// Every other `subscript_rt_(str|arr|json|regex|assoc|worker)_` entry,
/// with the reason a script cannot size its result.
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
        "subscript_rt_str_to_upper",
        "the result is at most three bytes per receiver byte, and the quota holds the receiver",
    ),
    (
        "subscript_rt_str_to_lower",
        "the result is at most three bytes per receiver byte, and the quota holds the receiver",
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
        "subscript_rt_json_parse_begin",
        "the transient document follows the input string, which the quota holds",
    ),
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
    (
        "subscript_rt_arr_sort",
        "the two temporaries are copies of the receiver, which the quota holds",
    ),
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
        let peak = peak_of("subscript_rt_str_repeat", &mut ctx, |_| {
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
        let peak = peak_of(entry, &mut ctx, |_| {
            // SAFETY: live exclusive Context and live string handles.
            unsafe { call(pointer, receiver, HUGE as i32, pad, 1) };
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
        let peak = peak_of("subscript_rt_str_replace", &mut ctx, |_| {
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
        let peak = peak_of("subscript_rt_str_replace_all", &mut ctx, |_| {
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
        let peak = peak_of("subscript_rt_regex_replace", &mut ctx, |_| {
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
        let peak = peak_of("subscript_rt_regex_replace_all", &mut ctx, |_| {
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
        let peak = peak_of("subscript_rt_arr_join", &mut ctx, |_| {
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
        let peak = peak_of(entry, &mut ctx, |ctx| {
            for _ in 0..16_384 {
                // SAFETY: live exclusive Context, live builder, live
                // string handle.
                unsafe {
                    if quoted {
                        ffi::subscript_rt_json_str(pointer, builder, piece, 1);
                    } else {
                        ffi::subscript_rt_json_raw(pointer, builder, piece, 1);
                    }
                }
                if ctx.trapped() {
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
            "{}: peak {} bytes under a {QUOTA}-byte quota, trap {:?}",
            one.entry, one.peak, one.trap
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
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    // SAFETY: live exclusive Context and live string handle.
    let result = unsafe { ffi::subscript_rt_str_repeat(pointer, receiver, megabyte, 1) };
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(before);
    assert!(!result.is_null(), "no quota accepts the request");
    assert!(
        peak >= megabyte as usize,
        "the request must hold its result: {peak} bytes"
    );
}

#[test]
fn every_exported_entry_is_covered_or_listed_with_a_reason() {
    let _guard = one_at_a_time();
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ffi.rs"))
        .expect("read runtime/src/ffi.rs");
    let mut exported: Vec<&str> = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.strip_prefix("pub unsafe extern \"C\" fn ") else {
            continue;
        };
        let name = rest.split('(').next().unwrap_or_default();
        if name.strip_prefix("subscript_rt_").is_some_and(|tail| {
            ["str_", "arr_", "json_", "regex_", "assoc_", "worker_"]
                .iter()
                .any(|family| tail.starts_with(family))
        }) {
            exported.push(name);
        }
    }
    assert!(
        exported.len() > 90,
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
