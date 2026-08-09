//! `Date` runtime (stdlib.md §3): proleptic-Gregorian calendar
//! arithmetic over `i64` epoch milliseconds (UTC), implemented once and
//! called by both tiers through the opaque `subscript_rt_date_*` boundary in
//! [`crate::ffi`].
//!
//! The civil↔day conversions are the well-known public-domain
//! algorithms (`days_from_civil` / `civil_from_days`), restated over
//! `i64` with euclidean division so pre-1970 (negative) days are exact.
//! Day 0 is 1970-01-01 (a Thursday). Every decomposition of
//! milliseconds into days and time-of-day is euclidean: -1 ms is
//! 1969-12-31T23:59:59.999Z, never an off-by-one.
//!
//! Result semantics are ECMA-262's for the accepted subset (stdlib.md
//! §0.4): `Date.UTC` carries out-of-range month/day arithmetically
//! (month0 13 rolls into the next year, day 0 into the previous month
//! — ECMA MakeDay over integer arguments) and maps two-digit years 0–99
//! to 1900+year (ECMA MakeFullYear). Valid times are
//! `|ms| <= 8_640_000_000_000_000` (the ECMA TimeClip range); an
//! out-of-range result is reported by `None` here and becomes a trap at
//! the FFI boundary — there is no Invalid-Date value (Q20).

/// Milliseconds per day.
pub const MS_PER_DAY: i64 = 86_400_000;

/// The ECMA TimeClip bound: valid times satisfy `|ms| <= MAX_DATE_MS`.
pub const MAX_DATE_MS: i64 = 8_640_000_000_000_000;

/// `subscript_rt_date_get` field code: `getUTCFullYear`.
pub const FIELD_FULL_YEAR: u32 = 0;
/// `subscript_rt_date_get` field code: `getUTCMonth` (0-based).
pub const FIELD_MONTH: u32 = 1;
/// `subscript_rt_date_get` field code: `getUTCDate`.
pub const FIELD_DATE: u32 = 2;
/// `subscript_rt_date_get` field code: `getUTCDay` (0 = Sunday).
pub const FIELD_DAY: u32 = 3;
/// `subscript_rt_date_get` field code: `getUTCHours`.
pub const FIELD_HOURS: u32 = 4;
/// `subscript_rt_date_get` field code: `getUTCMinutes`.
pub const FIELD_MINUTES: u32 = 5;
/// `subscript_rt_date_get` field code: `getUTCSeconds`.
pub const FIELD_SECONDS: u32 = 6;
/// `subscript_rt_date_get` field code: `getUTCMilliseconds`.
pub const FIELD_MILLISECONDS: u32 = 7;

/// True when `ms` is a valid time value (ECMA TimeClip range).
#[must_use]
pub fn in_range(ms: i64) -> bool {
    ms.unsigned_abs() <= MAX_DATE_MS as u64
}

/// Days since 1970-01-01 of the civil date `(y, m, d)`; `m` is 1-based
/// (1 = January). `d` may lie outside the month (0, 32, …) and carries
/// arithmetically: the result is always
/// `days_from_civil(y, m, 1) + (d - 1)`.
#[must_use]
pub fn days_from_civil(y: i64, m: u32, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 }); // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date `(year, month 1-based, day)` of day number `z` (days
/// since 1970-01-01; negative for pre-1970).
#[must_use]
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Day of week of day number `z`: 0 = Sunday … 6 = Saturday. Euclidean
/// modulo, so pre-1970 (negative) days are correct (day 0 is Thursday,
/// so day -1 is Wednesday = 3).
#[must_use]
pub fn weekday_from_days(z: i64) -> u32 {
    (z + 4).rem_euclid(7) as u32
}

/// `Date.UTC(year, month0, day, hours, minutes, seconds, millis)` with
/// every argument present (the checker fills the defaults: day 1, time
/// components 0). ECMA semantics over integer arguments: MakeFullYear
/// maps 0–99 to 1900+year; out-of-range month0/day carry arithmetically
/// (euclidean year/month normalization, then continuous day offset).
/// `None` when the result falls outside the TimeClip range (the caller
/// traps, Q20).
#[must_use]
pub fn utc_ms(
    year: i32,
    month0: i32,
    day: i32,
    hours: i32,
    minutes: i32,
    seconds: i32,
    millis: i32,
) -> Option<i64> {
    // ECMA MakeFullYear: two-digit years select the 20th century.
    let year = i64::from(year);
    let year = if (0..=99).contains(&year) {
        1900 + year
    } else {
        year
    };
    let ym = year * 12 + i64::from(month0);
    let y = ym.div_euclid(12);
    let m = (ym.rem_euclid(12) + 1) as u32;
    let days = days_from_civil(y, m, 1) + (i64::from(day) - 1);
    // i128: an extreme i32 year overflows i64 milliseconds; the range
    // check below rejects everything outside TimeClip exactly.
    let ms = i128::from(days) * i128::from(MS_PER_DAY)
        + i128::from(hours) * 3_600_000
        + i128::from(minutes) * 60_000
        + i128::from(seconds) * 1_000
        + i128::from(millis);
    if ms.unsigned_abs() <= MAX_DATE_MS as u128 {
        Some(ms as i64)
    } else {
        None
    }
}

/// The UTC calendar fields of a millisecond time value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DateFields {
    /// Civil year (proleptic Gregorian; may be negative).
    pub year: i64,
    /// Month, 0-based (0 = January) — the lib's `getUTCMonth` view.
    pub month0: u32,
    /// Day of month, 1-based.
    pub day: u32,
    /// Day of week, 0 = Sunday.
    pub weekday: u32,
    /// Hours [0, 23].
    pub hours: u32,
    /// Minutes [0, 59].
    pub minutes: u32,
    /// Seconds [0, 59].
    pub seconds: u32,
    /// Milliseconds [0, 999].
    pub millis: u32,
}

/// Decomposes `ms` into its UTC calendar fields. The day/time split is
/// euclidean, so negative (pre-1970) times decompose exactly.
#[must_use]
pub fn decompose(ms: i64) -> DateFields {
    let days = ms.div_euclid(MS_PER_DAY);
    let tod = ms.rem_euclid(MS_PER_DAY); // [0, MS_PER_DAY)
    let (year, m, d) = civil_from_days(days);
    DateFields {
        year,
        month0: m - 1,
        day: d,
        weekday: weekday_from_days(days),
        hours: (tod / 3_600_000) as u32,
        minutes: (tod / 60_000 % 60) as u32,
        seconds: (tod / 1_000 % 60) as u32,
        millis: (tod % 1_000) as u32,
    }
}

/// One UTC accessor by its `FIELD_*` code (the `subscript_rt_date_get`
/// contract). `None` for an unknown code — the FFI boundary reports it
/// as an internal trap, never a panic.
#[must_use]
pub fn get_field(ms: i64, field: u32) -> Option<i32> {
    let f = decompose(ms);
    Some(match field {
        FIELD_FULL_YEAR => f.year as i32,
        FIELD_MONTH => f.month0 as i32,
        FIELD_DATE => f.day as i32,
        FIELD_DAY => f.weekday as i32,
        FIELD_HOURS => f.hours as i32,
        FIELD_MINUTES => f.minutes as i32,
        FIELD_SECONDS => f.seconds as i32,
        FIELD_MILLISECONDS => f.millis as i32,
        _ => return None,
    })
}

/// `toISOString()`: exactly `YYYY-MM-DDTHH:mm:ss.sssZ`, year zero-padded
/// to four digits, milliseconds to three. `None` when the year is
/// outside 0000–9999 (the caller traps, Q20).
#[must_use]
pub fn to_iso(ms: i64) -> Option<String> {
    let f = decompose(ms);
    if !(0..=9999).contains(&f.year) {
        return None;
    }
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        f.year,
        f.month0 + 1,
        f.day,
        f.hours,
        f.minutes,
        f.seconds,
        f.millis
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_day_zero_and_a_thursday() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(weekday_from_days(0), 4); // Thursday
        let f = decompose(0);
        assert_eq!((f.year, f.month0, f.day, f.weekday), (1970, 0, 1, 4));
        assert_eq!((f.hours, f.minutes, f.seconds, f.millis), (0, 0, 0, 0));
    }

    #[test]
    fn leap_rules_follow_the_400_year_cycle() {
        // 2000-02-29 exists (400 rule); the day after is March 1.
        let feb29 = days_from_civil(2000, 2, 29);
        assert_eq!(civil_from_days(feb29), (2000, 2, 29));
        assert_eq!(civil_from_days(feb29 + 1), (2000, 3, 1));
        // 1900 and 2100 are not leap years: day 29 carries to March 1.
        assert_eq!(civil_from_days(days_from_civil(1900, 2, 29)), (1900, 3, 1));
        assert_eq!(civil_from_days(days_from_civil(2100, 2, 29)), (2100, 3, 1));
        // 2400 is leap again.
        assert_eq!(civil_from_days(days_from_civil(2400, 2, 29)), (2400, 2, 29));
    }

    #[test]
    fn pre_1970_days_are_exact() {
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(weekday_from_days(-1), 3); // Wednesday
    }

    #[test]
    fn minus_one_millisecond_is_the_last_instant_of_1969() {
        // The euclidean day/time split, pinned exactly (stdlib.md §3).
        let f = decompose(-1);
        assert_eq!((f.year, f.month0, f.day), (1969, 11, 31));
        assert_eq!((f.hours, f.minutes, f.seconds, f.millis), (23, 59, 59, 999));
        assert_eq!(to_iso(-1).as_deref(), Some("1969-12-31T23:59:59.999Z"));
    }

    #[test]
    fn civil_round_trips_across_a_wide_year_sweep() {
        // Every day of 1600..2400 round-trips days→civil→days.
        let start = days_from_civil(1600, 1, 1);
        let end = days_from_civil(2400, 12, 31);
        for z in start..=end {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(
                days_from_civil(y, m, i64::from(d)),
                z,
                "round trip at day {z}"
            );
        }
        // Spot years at the toISOString bounds.
        for y in [0i64, 9999] {
            let z = days_from_civil(y, 1, 1);
            assert_eq!(civil_from_days(z), (y, 1, 1), "year {y}");
        }
    }

    #[test]
    fn known_weekdays() {
        // 2000-01-01 was a Saturday; 2020-06-15 a Monday.
        assert_eq!(weekday_from_days(days_from_civil(2000, 1, 1)), 6);
        assert_eq!(weekday_from_days(days_from_civil(2020, 6, 15)), 1);
    }

    #[test]
    fn utc_ms_defaults_and_full_fields() {
        assert_eq!(utc_ms(1970, 0, 1, 0, 0, 0, 0), Some(0));
        assert_eq!(
            utc_ms(2020, 5, 15, 12, 34, 56, 789),
            Some(1_592_224_496_789)
        );
        assert_eq!(
            to_iso(1_592_224_496_789).as_deref(),
            Some("2020-06-15T12:34:56.789Z")
        );
    }

    #[test]
    fn utc_ms_carries_month_and_day_arithmetically() {
        // month0 13 rolls into February of the next year.
        assert_eq!(
            utc_ms(2020, 13, 1, 0, 0, 0, 0),
            utc_ms(2021, 1, 1, 0, 0, 0, 0)
        );
        // day 0 is the last day of the previous month (2020 is leap).
        assert_eq!(
            utc_ms(2020, 2, 0, 0, 0, 0, 0),
            utc_ms(2020, 1, 29, 0, 0, 0, 0)
        );
        // 1900-02-29 does not exist; it is 1900-03-01.
        assert_eq!(
            utc_ms(1900, 1, 29, 0, 0, 0, 0),
            utc_ms(1900, 2, 1, 0, 0, 0, 0)
        );
        // Negative month0 carries backwards.
        assert_eq!(
            utc_ms(2020, -1, 1, 0, 0, 0, 0),
            utc_ms(2019, 11, 1, 0, 0, 0, 0)
        );
    }

    #[test]
    fn utc_ms_maps_two_digit_years_to_1900() {
        // ECMA MakeFullYear: 0..=99 → 1900+year.
        let ms = utc_ms(7, 0, 1, 0, 0, 0, 0).expect("in range");
        assert_eq!(decompose(ms).year, 1907);
        assert_eq!(utc_ms(0, 0, 1, 0, 0, 0, 0), utc_ms(1900, 0, 1, 0, 0, 0, 0));
        // 100 is not two-digit.
        let ms = utc_ms(100, 0, 1, 0, 0, 0, 0).expect("in range");
        assert_eq!(decompose(ms).year, 100);
    }

    #[test]
    fn utc_ms_rejects_results_outside_the_time_clip_range() {
        // +275760-09-13T00:00:00Z is exactly MAX_DATE_MS.
        assert_eq!(utc_ms(275_760, 8, 13, 0, 0, 0, 0), Some(MAX_DATE_MS));
        assert_eq!(utc_ms(275_760, 8, 13, 0, 0, 0, 1), None);
        assert_eq!(utc_ms(275_760, 8, 14, 0, 0, 0, 0), None);
        // -271821-04-20T00:00:00Z is exactly -MAX_DATE_MS.
        assert_eq!(utc_ms(-271_821, 3, 20, 0, 0, 0, 0), Some(-MAX_DATE_MS));
        assert_eq!(utc_ms(-271_821, 3, 19, 0, 0, 0, 0), None);
        // An extreme year must not wrap i64 silently.
        assert_eq!(utc_ms(i32::MAX, 0, 1, 0, 0, 0, 0), None);
        assert_eq!(utc_ms(i32::MIN, 0, 1, 0, 0, 0, 0), None);
    }

    #[test]
    fn in_range_matches_the_time_clip_bound() {
        assert!(in_range(0));
        assert!(in_range(MAX_DATE_MS));
        assert!(in_range(-MAX_DATE_MS));
        assert!(!in_range(MAX_DATE_MS + 1));
        assert!(!in_range(-MAX_DATE_MS - 1));
        assert!(!in_range(i64::MIN));
    }

    #[test]
    fn get_field_covers_the_eight_codes_and_rejects_unknown_ones() {
        let ms = utc_ms(2020, 5, 15, 12, 34, 56, 789).expect("in range");
        assert_eq!(get_field(ms, FIELD_FULL_YEAR), Some(2020));
        assert_eq!(get_field(ms, FIELD_MONTH), Some(5));
        assert_eq!(get_field(ms, FIELD_DATE), Some(15));
        assert_eq!(get_field(ms, FIELD_DAY), Some(1));
        assert_eq!(get_field(ms, FIELD_HOURS), Some(12));
        assert_eq!(get_field(ms, FIELD_MINUTES), Some(34));
        assert_eq!(get_field(ms, FIELD_SECONDS), Some(56));
        assert_eq!(get_field(ms, FIELD_MILLISECONDS), Some(789));
        assert_eq!(get_field(ms, 8), None);
    }

    #[test]
    fn to_iso_pads_and_enforces_the_year_range() {
        assert_eq!(to_iso(0).as_deref(), Some("1970-01-01T00:00:00.000Z"));
        // Year 7, zero-padded to four digits.
        let y7 = days_from_civil(7, 3, 4) * MS_PER_DAY;
        assert_eq!(y7, -61_940_937_600_000);
        assert_eq!(to_iso(y7).as_deref(), Some("0007-03-04T00:00:00.000Z"));
        // The year bounds.
        let y0 = days_from_civil(0, 1, 1) * MS_PER_DAY;
        assert_eq!(to_iso(y0).as_deref(), Some("0000-01-01T00:00:00.000Z"));
        let y9999 = days_from_civil(9999, 12, 31) * MS_PER_DAY + MS_PER_DAY - 1;
        assert_eq!(y9999, 253_402_300_799_999);
        assert_eq!(to_iso(y9999).as_deref(), Some("9999-12-31T23:59:59.999Z"));
        // Out of the printable range: one ms past either bound.
        assert_eq!(to_iso(y0 - 1), None);
        assert_eq!(to_iso(y9999 + 1), None);
        // The TimeClip extremes are valid times but not printable.
        assert_eq!(to_iso(MAX_DATE_MS), None);
        assert_eq!(to_iso(-MAX_DATE_MS), None);
    }

    #[test]
    fn time_clip_extremes_decompose_to_the_known_calendar_dates() {
        let max = decompose(MAX_DATE_MS);
        assert_eq!(
            (max.year, max.month0, max.day, max.weekday),
            (275_760, 8, 13, 6)
        );
        let min = decompose(-MAX_DATE_MS);
        assert_eq!(
            (min.year, min.month0, min.day, min.weekday),
            (-271_821, 3, 20, 2)
        );
    }
}
