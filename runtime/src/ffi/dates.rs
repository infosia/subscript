use crate::context::Context;
use crate::trap::TrapKind;

// ----- Date (stdlib.md §3) -----
//
// One implementation, both tiers, through these opaque symbols (never a
// direct libc time call in generated code). A `Date` value crosses this
// boundary as its `i64` epoch-millisecond representation. The trapping
// entries (`utc`, `new`, `to_iso`) carry a trailing `pos_id`; the pure
// accessors do not trap and take none.

/// `Date.UTC(y, m0, d, h, min, s, ms)` — the checker always supplies
/// all seven arguments (missing trailing ones default to day 1 / time
/// 0 at check time). ECMA MakeDay/MakeFullYear semantics; a result
/// outside the TimeClip range traps (Q20) and returns 0.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_utc(
    ctx: *mut Context,
    year: i32,
    month0: i32,
    day: i32,
    hours: i32,
    minutes: i32,
    seconds: i32,
    millis: i32,
    pos_id: u32,
) -> i64 {
    match crate::date::utc_ms(year, month0, day, hours, minutes, seconds, millis) {
        Some(ms) => ms,
        None => {
            // SAFETY: shared contract.
            unsafe { &mut *ctx }.trap(
                TrapKind::DateRange,
                "Date out of range: Date.UTC result exceeds the valid time range \
                 (|ms| <= 8640000000000000)",
                pos_id,
            );
            0
        }
    }
}

/// `new Date(ms)`: the identity on an in-range time value; out of the
/// TimeClip range traps (Q20 — no Invalid-Date value) and returns 0.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_new(ctx: *mut Context, ms: i64, pos_id: u32) -> i64 {
    if crate::date::in_range(ms) {
        return ms;
    }
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.trap(
        TrapKind::DateRange,
        format!("Date out of range: {ms} ms (valid: |ms| <= 8640000000000000)"),
        pos_id,
    );
    0
}

/// `Date.now()`: current UTC milliseconds from the Context clock —
/// pinned by [`subscript_rt_ctx_set_now`], else the system clock.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_now(ctx: *mut Context) -> i64 {
    // SAFETY: shared contract.
    unsafe { &*ctx }.now_utc_ms()
}

/// One UTC accessor on a Date's millisecond value, selected by its
/// `FIELD_*` code ([`crate::date`]). An unknown code is a
/// compiler/runtime disagreement: reported as an internal trap, never
/// a panic.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_get(ctx: *mut Context, ms: i64, field: u32) -> i32 {
    match crate::date::get_field(ms, field) {
        Some(v) => v,
        None => {
            // SAFETY: shared contract.
            unsafe { &mut *ctx }.trap(
                TrapKind::Internal,
                format!("unknown Date accessor field code {field}"),
                0,
            );
            0
        }
    }
}

/// `toISOString()`: allocates the `YYYY-MM-DDTHH:mm:ss.sssZ` string;
/// a year outside 0000–9999 traps (Q20) and returns null.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_to_iso(
    ctx: *mut Context,
    ms: i64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match crate::date::to_iso(ms) {
        Some(s) => ctx.alloc_str(s.as_bytes(), pos_id),
        None => {
            let year = crate::date::decompose(ms).year;
            ctx.trap(
                TrapKind::DateRange,
                format!("toISOString requires a year in 0000-9999, got year {year}"),
                pos_id,
            );
            std::ptr::null_mut()
        }
    }
}

/// `toUTCString()`: allocates UTC text for every TimeClip year.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_to_utc_string(
    ctx: *mut Context,
    ms: i64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::date::to_utc_string(ms).as_bytes(), pos_id)
}
