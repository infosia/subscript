//! Trap kinds and trap records.
//!
//! A trap is a runtime fault (collisions.md C6): the script run stops
//! with a diagnostic, the host process survives, and no signal or SEH
//! mechanism is involved. The mechanism: every runtime function that
//! detects a fault records a [`TrapRecord`] on the Context and raises
//! the Context's trap flag; generated code checks the flag after every
//! call (and after its own emitted checks) and unwinds by returning
//! early from every active script frame. The JIT entry then reads the
//! record. See `Context` for the flag's layout guarantee.

use std::fmt;

/// The kind of runtime fault. Values are stable across the C boundary
/// (generated code passes them to the trap entry point as `u32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[repr(u32)]
pub enum TrapKind {
    /// Array or `FixedArray` index out of bounds.
    IndexOutOfBounds = 1,
    /// `pop()` or `shift()` on an empty array.
    EmptyPop = 2,
    /// `string.slice` range invalid or off a UTF-8 boundary (Q5).
    StringSlice = 3,
    /// Checked `as` narrowing applied to `null` (C3).
    NullNarrowing = 4,
    /// Checked `as` narrowing to a class the instance does not have.
    ClassMismatch = 5,
    /// `Context.free` of an allocation that was already deleted (Q6).
    DoubleDelete = 6,
    /// Access through a reference whose allocation was deleted (Q6).
    UseAfterDelete = 7,
    /// `Context.free` of a pointer the Context does not own.
    InvalidDelete = 8,
    /// Context allocation failure.
    AllocationFailure = 9,
    /// Integer division or remainder by zero.
    DivisionByZero = 10,
    /// Internal inconsistency (e.g. an unknown trap kind crossed the
    /// C boundary); always a compiler/runtime bug, never a program
    /// fault.
    Internal = 11,
    /// Resume of a coroutine suspended in a function body that a hot
    /// reload replaced (`specs/blocks/compiler.md` §8.2).
    StaleCoroutine = 12,
    /// A `Date` outside its valid range (stdlib.md §3, Q20): a time
    /// value beyond the ECMA TimeClip bound, or `toISOString` on a year
    /// outside 0000–9999. There is no Invalid-Date value.
    DateRange = 13,
    /// A `String` method range or argument error (stdlib.md §8, Q21):
    /// `charCodeAt` out of range, a negative `repeat` count, an empty
    /// `split` separator, an empty `replaceAll` pattern, or an empty
    /// pad that cannot reach the target length. JS returns NaN or a
    /// silent no-op in these cases; the language traps.
    StrRange = 14,
    /// A `Number` operation received a programmer-error range
    /// (stdlib.md §11, Q25): a `parseInt` radix outside 2–36 or a
    /// `toFixed` digit count outside 0–100.
    NumberRange = 15,
    /// `JSON.stringify` received NaN or either infinity (stdlib.md
    /// §13.3): unlike JavaScript's lossy `null`, the language traps.
    JsonNumber = 16,
    /// `JSON.stringify` revisited a reference already on the active
    /// serialization path, proving a cyclic object graph.
    JsonCycle = 17,
    /// A program read `JsonResult<T>.value` after parsing or static-type
    /// validation failed and `ok` was false.
    JsonResultValue = 18,
    /// A malformed pattern/flag set or a regex operation that violates
    /// its contracted flag requirements.
    Regex = 19,
    /// A regex search exhausted the host-configured Context budget.
    RegexBudget = 20,
    /// A registered callback fired with a userdata slot that no longer
    /// names a live Context allocation.
    CallbackUserdataFreed = 21,
    /// Joining a runtime worker whose dedicated Context trapped.
    WorkerTrapped = 22,
    /// Execution reached an `unreachable()` call statement.
    UnreachableReached = 23,
}

impl TrapKind {
    /// Decodes the stable `u32` form; unknown values map to `None`.
    #[must_use]
    pub fn from_u32(v: u32) -> Option<TrapKind> {
        Some(match v {
            1 => TrapKind::IndexOutOfBounds,
            2 => TrapKind::EmptyPop,
            3 => TrapKind::StringSlice,
            4 => TrapKind::NullNarrowing,
            5 => TrapKind::ClassMismatch,
            6 => TrapKind::DoubleDelete,
            7 => TrapKind::UseAfterDelete,
            8 => TrapKind::InvalidDelete,
            9 => TrapKind::AllocationFailure,
            10 => TrapKind::DivisionByZero,
            11 => TrapKind::Internal,
            12 => TrapKind::StaleCoroutine,
            13 => TrapKind::DateRange,
            14 => TrapKind::StrRange,
            15 => TrapKind::NumberRange,
            16 => TrapKind::JsonNumber,
            17 => TrapKind::JsonCycle,
            18 => TrapKind::JsonResultValue,
            19 => TrapKind::Regex,
            20 => TrapKind::RegexBudget,
            21 => TrapKind::CallbackUserdataFreed,
            22 => TrapKind::WorkerTrapped,
            23 => TrapKind::UnreachableReached,
            _ => return None,
        })
    }

    /// Stable rule name used in reports.
    #[must_use]
    pub fn rule(self) -> &'static str {
        match self {
            TrapKind::IndexOutOfBounds => "index-out-of-bounds",
            TrapKind::EmptyPop => "empty-pop",
            TrapKind::StringSlice => "string-slice",
            TrapKind::NullNarrowing => "null-narrowing",
            TrapKind::ClassMismatch => "class-mismatch",
            TrapKind::DoubleDelete => "double-delete",
            TrapKind::UseAfterDelete => "use-after-delete",
            TrapKind::InvalidDelete => "invalid-delete",
            TrapKind::AllocationFailure => "allocation-failure",
            TrapKind::DivisionByZero => "division-by-zero",
            TrapKind::Internal => "internal",
            TrapKind::StaleCoroutine => "stale-coroutine-after-reload",
            TrapKind::DateRange => "date-range",
            TrapKind::StrRange => "string-range",
            TrapKind::NumberRange => "number-range",
            TrapKind::JsonNumber => "json-non-finite-number",
            TrapKind::JsonCycle => "json-cycle",
            TrapKind::JsonResultValue => "json-result-value",
            TrapKind::Regex => "regex-error",
            TrapKind::RegexBudget => "regex-budget-exhausted",
            TrapKind::CallbackUserdataFreed => "callback-userdata-freed",
            TrapKind::WorkerTrapped => "worker-trapped",
            TrapKind::UnreachableReached => "unreachable-reached",
        }
    }
}

impl fmt::Display for TrapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.rule())
    }
}

/// One recorded trap: kind, message, and the compiler-assigned index
/// into the position table embedded by the code generator.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TrapRecord {
    /// The fault kind.
    pub kind: TrapKind,
    /// Human-readable detail.
    pub message: String,
    /// Index into the compiler's position table (maps to a TS position).
    pub pos_id: u32,
}

impl TrapRecord {
    /// Builds a record.
    #[must_use]
    pub fn new(kind: TrapKind, message: impl Into<String>, pos_id: u32) -> Self {
        TrapRecord {
            kind,
            message: message.into(),
            pos_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_u32() {
        for v in 1..=23u32 {
            let k = TrapKind::from_u32(v).expect("known kind");
            assert_eq!(k as u32, v);
        }
        assert_eq!(TrapKind::from_u32(0), None);
        assert_eq!(TrapKind::from_u32(99), None);
    }

    #[test]
    fn display_is_the_rule_name() {
        assert_eq!(TrapKind::IndexOutOfBounds.to_string(), "index-out-of-bounds");
    }

    #[test]
    fn regex_error_kind_has_stable_number_and_rule() {
        assert_eq!(TrapKind::Regex as u32, 19);
        assert_eq!(TrapKind::from_u32(19), Some(TrapKind::Regex));
        assert_eq!(TrapKind::Regex.rule(), "regex-error");
    }

    #[test]
    fn worker_trapped_kind_has_stable_number_and_rule() {
        assert_eq!(TrapKind::WorkerTrapped as u32, 22);
        assert_eq!(TrapKind::from_u32(22), Some(TrapKind::WorkerTrapped));
        assert_eq!(TrapKind::WorkerTrapped.rule(), "worker-trapped");
    }

    #[test]
    fn unreachable_reached_kind_has_stable_number_and_rule() {
        assert_eq!(TrapKind::UnreachableReached as u32, 23);
        assert_eq!(
            TrapKind::from_u32(23),
            Some(TrapKind::UnreachableReached)
        );
        assert_eq!(TrapKind::UnreachableReached.rule(), "unreachable-reached");
    }

    #[test]
    fn record_carries_kind_message_pos() {
        let r = TrapRecord::new(TrapKind::EmptyPop, "pop on empty array", 7);
        assert_eq!(r.kind, TrapKind::EmptyPop);
        assert_eq!(r.pos_id, 7);
        assert!(r.message.contains("empty"));
    }
}
