//! The dev-tier JIT driver: instantiates the tier-neutral lowering
//! with `cranelift-jit`, resolves the runtime's `extern "C"` symbols,
//! runs the exported `main(): void`, and returns the captured stdout
//! bytes or a trap report.

use std::time::{Duration, Instant};

use cranelift_jit::{JITBuilder, JITModule};
use subscript_compiler::{check_program, Diagnostic, Pos, SourceFile};
use subscript_runtime::{
    ffi, Context, TrapKind, FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES,
};

use crate::lower::{dev_flags, internal, lower_module_with, Lowered, LowerOptions};
use crate::native::{missing_symbol, register_symbols};
use crate::NativeLibrary;

/// A runtime fault that stopped the script (collisions.md C6). The
/// host process survives; this is the report the Context recorded.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TrapReport {
    /// The violated rule.
    pub rule: TrapKind,
    /// Human-readable detail.
    pub message: String,
    /// TS position of the faulting construct (from the position table
    /// the compiler embeds).
    pub pos: Pos,
    /// Exact stdout bytes produced before the Context stopped.
    pub stdout: Vec<u8>,
}

/// Context memory accounting observed after a successful dev-tier run.
///
/// Both figures follow the tier-specific policy in `compiler.md` §18.2d.
/// The development tier reports exact requested payload bytes as live and
/// includes retained-and-poisoned allocation layouts in reserved bytes when
/// freed-handle diagnostics are enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct JitMemoryAccounting {
    /// Requested payload bytes in live Context-owned allocations after the
    /// run, following §18.2d's exact-requested-bytes development-tier policy.
    pub live_bytes: u64,
    /// Bytes still reserved by the Context after the run, including retained
    /// and poisoned layouts when §8.1a-3's freed-handle diagnostics are
    /// enabled with threshold 0 and the recommended default budget.
    pub reserved_bytes: u64,
}

impl std::fmt::Display for TrapReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: trap [{}]: {}", self.pos, self.rule, self.message)
    }
}

/// Why a run did not complete normally.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunError {
    /// The program was rejected by the checker (P1 diagnostics).
    Rejected(Vec<Diagnostic>),
    /// The program ran and trapped.
    Trap(TrapReport),
    /// A called foreign C symbol was absent from every caller-supplied
    /// native library.
    UnresolvedForeignSymbol(String),
    /// An internal lowering/backend failure (a bug, not a user error).
    Internal(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Rejected(diags) => {
                write!(f, "rejected with {} diagnostic(s)", diags.len())?;
                for d in diags {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
            RunError::Trap(t) => write!(f, "{t}"),
            RunError::UnresolvedForeignSymbol(name) => write!(
                f,
                "unresolved foreign symbol `{name}`: no supplied native library registers it"
            ),
            RunError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Registers every runtime symbol the lowering imports.
///
/// The dev tier binds them by address; the ship tier resolves the same
/// names from the runtime static library at link time, so this list and
/// the lowering's imports must stay in step with `runtime::ffi`.
pub(crate) fn register_runtime(builder: &mut JITBuilder) {
    let syms: &[(&str, *const u8)] = &[
        ("subscript_rt_print", ffi::subscript_rt_print as *const u8),
        ("subscript_rt_collect", ffi::subscript_rt_collect as *const u8),
        ("subscript_rt_alloc", ffi::subscript_rt_alloc as *const u8),
        (
            "subscript_rt_async_kick",
            ffi::subscript_rt_async_kick as *const u8,
        ),
        ("subscript_rt_delete", ffi::subscript_rt_delete as *const u8),
        ("subscript_rt_trap", ffi::subscript_rt_trap as *const u8),
        ("subscript_rt_root_add", ffi::subscript_rt_root_add as *const u8),
        ("subscript_rt_shadow_push", ffi::subscript_rt_shadow_push as *const u8),
        ("subscript_rt_shadow_pop", ffi::subscript_rt_shadow_pop as *const u8),
        ("subscript_rt_str_lit", ffi::subscript_rt_str_lit as *const u8),
        ("subscript_rt_str_len", ffi::subscript_rt_str_len as *const u8),
        ("subscript_rt_str_concat", ffi::subscript_rt_str_concat as *const u8),
        ("subscript_rt_str_slice", ffi::subscript_rt_str_slice as *const u8),
        ("subscript_rt_str_eq", ffi::subscript_rt_str_eq as *const u8),
        ("subscript_rt_fmt_i32", ffi::subscript_rt_fmt_i32 as *const u8),
        ("subscript_rt_fmt_u32", ffi::subscript_rt_fmt_u32 as *const u8),
        ("subscript_rt_fmt_i64", ffi::subscript_rt_fmt_i64 as *const u8),
        ("subscript_rt_fmt_u64", ffi::subscript_rt_fmt_u64 as *const u8),
        ("subscript_rt_fmt_f32", ffi::subscript_rt_fmt_f32 as *const u8),
        ("subscript_rt_fmt_f64", ffi::subscript_rt_fmt_f64 as *const u8),
        ("subscript_rt_fmt_bool", ffi::subscript_rt_fmt_bool as *const u8),
        ("subscript_rt_json_begin", ffi::subscript_rt_json_begin as *const u8),
        (
            "subscript_rt_json_begin_tracked",
            ffi::subscript_rt_json_begin_tracked as *const u8,
        ),
        ("subscript_rt_json_finish", ffi::subscript_rt_json_finish as *const u8),
        ("subscript_rt_json_raw", ffi::subscript_rt_json_raw as *const u8),
        ("subscript_rt_json_str", ffi::subscript_rt_json_str as *const u8),
        ("subscript_rt_json_i32", ffi::subscript_rt_json_i32 as *const u8),
        ("subscript_rt_json_u32", ffi::subscript_rt_json_u32 as *const u8),
        ("subscript_rt_json_i64", ffi::subscript_rt_json_i64 as *const u8),
        ("subscript_rt_json_u64", ffi::subscript_rt_json_u64 as *const u8),
        ("subscript_rt_json_f32", ffi::subscript_rt_json_f32 as *const u8),
        ("subscript_rt_json_f64", ffi::subscript_rt_json_f64 as *const u8),
        ("subscript_rt_json_bool", ffi::subscript_rt_json_bool as *const u8),
        ("subscript_rt_json_date", ffi::subscript_rt_json_date as *const u8),
        ("subscript_rt_json_null", ffi::subscript_rt_json_null as *const u8),
        ("subscript_rt_json_visit", ffi::subscript_rt_json_visit as *const u8),
        ("subscript_rt_json_leave", ffi::subscript_rt_json_leave as *const u8),
        (
            "subscript_rt_json_parse_begin",
            ffi::subscript_rt_json_parse_begin as *const u8,
        ),
        (
            "subscript_rt_json_parse_end",
            ffi::subscript_rt_json_parse_end as *const u8,
        ),
        (
            "subscript_rt_json_parse_root",
            ffi::subscript_rt_json_parse_root as *const u8,
        ),
        (
            "subscript_rt_json_parse_is_kind",
            ffi::subscript_rt_json_parse_is_kind as *const u8,
        ),
        (
            "subscript_rt_json_parse_number_fits",
            ffi::subscript_rt_json_parse_number_fits as *const u8,
        ),
        (
            "subscript_rt_json_parse_number",
            ffi::subscript_rt_json_parse_number as *const u8,
        ),
        (
            "subscript_rt_json_parse_integer",
            ffi::subscript_rt_json_parse_integer as *const u8,
        ),
        (
            "subscript_rt_json_parse_bool",
            ffi::subscript_rt_json_parse_bool as *const u8,
        ),
        (
            "subscript_rt_json_parse_string",
            ffi::subscript_rt_json_parse_string as *const u8,
        ),
        (
            "subscript_rt_json_parse_array_len",
            ffi::subscript_rt_json_parse_array_len as *const u8,
        ),
        (
            "subscript_rt_json_parse_array_get",
            ffi::subscript_rt_json_parse_array_get as *const u8,
        ),
        (
            "subscript_rt_json_parse_object_get",
            ffi::subscript_rt_json_parse_object_get as *const u8,
        ),
        ("subscript_rt_f16_from_f64", ffi::subscript_rt_f16_from_f64 as *const u8),
        ("subscript_rt_f16_to_f64", ffi::subscript_rt_f16_to_f64 as *const u8),
        ("subscript_rt_array_new", ffi::subscript_rt_array_new as *const u8),
        ("subscript_rt_array_len", ffi::subscript_rt_array_len as *const u8),
        ("subscript_rt_array_push", ffi::subscript_rt_array_push as *const u8),
        ("subscript_rt_array_pop", ffi::subscript_rt_array_pop as *const u8),
        ("subscript_rt_array_ptr", ffi::subscript_rt_array_ptr as *const u8),
        (
            "subscript_rt_assoc_iter_begin",
            ffi::subscript_rt_assoc_iter_begin as *const u8,
        ),
        (
            "subscript_rt_assoc_iter_copy",
            ffi::subscript_rt_assoc_iter_copy as *const u8,
        ),
        (
            "subscript_rt_assoc_iter_end",
            ffi::subscript_rt_assoc_iter_end as *const u8,
        ),
        (
            "subscript_rt_str_iter_code_point",
            ffi::subscript_rt_str_iter_code_point as *const u8,
        ),
        (
            "subscript_rt_array_spread_array",
            ffi::subscript_rt_array_spread_array as *const u8,
        ),
        (
            "subscript_rt_array_spread_fixed",
            ffi::subscript_rt_array_spread_fixed as *const u8,
        ),
        (
            "subscript_rt_array_spread_assoc",
            ffi::subscript_rt_array_spread_assoc as *const u8,
        ),
        (
            "subscript_rt_array_spread_string",
            ffi::subscript_rt_array_spread_string as *const u8,
        ),
        ("subscript_rt_str_data", ffi::subscript_rt_str_data as *const u8),
        ("subscript_rt_array_data", ffi::subscript_rt_array_data as *const u8),
        ("subscript_rt_cb_bind", ffi::subscript_rt_cb_bind as *const u8),
        ("subscript_rt_cb_trampoline", ffi::subscript_rt_cb_trampoline as *const u8),
        // Math intrinsics (stdlib.md §1): the ship tier resolves the
        // same opaque symbols from the runtime static library.
        ("subscript_rt_math_abs", ffi::subscript_rt_math_abs as *const u8),
        ("subscript_rt_math_acos", ffi::subscript_rt_math_acos as *const u8),
        ("subscript_rt_math_acosh", ffi::subscript_rt_math_acosh as *const u8),
        ("subscript_rt_math_asin", ffi::subscript_rt_math_asin as *const u8),
        ("subscript_rt_math_asinh", ffi::subscript_rt_math_asinh as *const u8),
        ("subscript_rt_math_atan", ffi::subscript_rt_math_atan as *const u8),
        ("subscript_rt_math_atanh", ffi::subscript_rt_math_atanh as *const u8),
        ("subscript_rt_math_cbrt", ffi::subscript_rt_math_cbrt as *const u8),
        ("subscript_rt_math_ceil", ffi::subscript_rt_math_ceil as *const u8),
        ("subscript_rt_math_cos", ffi::subscript_rt_math_cos as *const u8),
        ("subscript_rt_math_cosh", ffi::subscript_rt_math_cosh as *const u8),
        ("subscript_rt_math_exp", ffi::subscript_rt_math_exp as *const u8),
        ("subscript_rt_math_expm1", ffi::subscript_rt_math_expm1 as *const u8),
        ("subscript_rt_math_floor", ffi::subscript_rt_math_floor as *const u8),
        ("subscript_rt_math_log", ffi::subscript_rt_math_log as *const u8),
        ("subscript_rt_math_log1p", ffi::subscript_rt_math_log1p as *const u8),
        ("subscript_rt_math_log10", ffi::subscript_rt_math_log10 as *const u8),
        ("subscript_rt_math_log2", ffi::subscript_rt_math_log2 as *const u8),
        ("subscript_rt_math_round", ffi::subscript_rt_math_round as *const u8),
        ("subscript_rt_math_sign", ffi::subscript_rt_math_sign as *const u8),
        ("subscript_rt_math_sin", ffi::subscript_rt_math_sin as *const u8),
        ("subscript_rt_math_sinh", ffi::subscript_rt_math_sinh as *const u8),
        ("subscript_rt_math_sqrt", ffi::subscript_rt_math_sqrt as *const u8),
        ("subscript_rt_math_tan", ffi::subscript_rt_math_tan as *const u8),
        ("subscript_rt_math_tanh", ffi::subscript_rt_math_tanh as *const u8),
        ("subscript_rt_math_trunc", ffi::subscript_rt_math_trunc as *const u8),
        ("subscript_rt_math_atan2", ffi::subscript_rt_math_atan2 as *const u8),
        ("subscript_rt_math_hypot", ffi::subscript_rt_math_hypot as *const u8),
        ("subscript_rt_math_pow", ffi::subscript_rt_math_pow as *const u8),
        ("subscript_rt_math_max", ffi::subscript_rt_math_max as *const u8),
        ("subscript_rt_math_min", ffi::subscript_rt_math_min as *const u8),
        ("subscript_rt_math_random", ffi::subscript_rt_math_random as *const u8),
        ("subscript_rt_math_clz32", ffi::subscript_rt_math_clz32 as *const u8),
        ("subscript_rt_math_imul", ffi::subscript_rt_math_imul as *const u8),
        ("subscript_rt_math_fround", ffi::subscript_rt_math_fround as *const u8),
        // Number and parsing intrinsics (stdlib.md §11, Q25/Q26).
        ("subscript_rt_num_is_nan", ffi::subscript_rt_num_is_nan as *const u8),
        (
            "subscript_rt_num_is_finite",
            ffi::subscript_rt_num_is_finite as *const u8,
        ),
        (
            "subscript_rt_num_is_integer",
            ffi::subscript_rt_num_is_integer as *const u8,
        ),
        (
            "subscript_rt_num_is_safe_integer",
            ffi::subscript_rt_num_is_safe_integer as *const u8,
        ),
        (
            "subscript_rt_num_parse_int",
            ffi::subscript_rt_num_parse_int as *const u8,
        ),
        (
            "subscript_rt_num_parse_float",
            ffi::subscript_rt_num_parse_float as *const u8,
        ),
        (
            "subscript_rt_num_to_fixed",
            ffi::subscript_rt_num_to_fixed as *const u8,
        ),
        (
            "subscript_rt_num_to_string_f32",
            ffi::subscript_rt_num_to_string_f32 as *const u8,
        ),
        (
            "subscript_rt_num_to_string_f64",
            ffi::subscript_rt_num_to_string_f64 as *const u8,
        ),
        (
            "subscript_rt_num_to_exponential",
            ffi::subscript_rt_num_to_exponential as *const u8,
        ),
        (
            "subscript_rt_num_to_precision",
            ffi::subscript_rt_num_to_precision as *const u8,
        ),
        // Date intrinsics (stdlib.md §3): same opaque-symbol rule; the
        // ship tier resolves these from the runtime static library.
        // String method intrinsics (stdlib.md §8): one opaque symbol
        // per accepted method, StrFn::ALL order.
        ("subscript_rt_str_index_of", ffi::subscript_rt_str_index_of as *const u8),
        (
            "subscript_rt_str_last_index_of",
            ffi::subscript_rt_str_last_index_of as *const u8,
        ),
        ("subscript_rt_str_includes", ffi::subscript_rt_str_includes as *const u8),
        (
            "subscript_rt_str_starts_with",
            ffi::subscript_rt_str_starts_with as *const u8,
        ),
        ("subscript_rt_str_ends_with", ffi::subscript_rt_str_ends_with as *const u8),
        (
            "subscript_rt_str_char_code_at",
            ffi::subscript_rt_str_char_code_at as *const u8,
        ),
        ("subscript_rt_str_split", ffi::subscript_rt_str_split as *const u8),
        ("subscript_rt_str_trim", ffi::subscript_rt_str_trim as *const u8),
        ("subscript_rt_str_trim_start", ffi::subscript_rt_str_trim_start as *const u8),
        ("subscript_rt_str_trim_end", ffi::subscript_rt_str_trim_end as *const u8),
        ("subscript_rt_str_repeat", ffi::subscript_rt_str_repeat as *const u8),
        ("subscript_rt_str_pad_start", ffi::subscript_rt_str_pad_start as *const u8),
        ("subscript_rt_str_pad_end", ffi::subscript_rt_str_pad_end as *const u8),
        ("subscript_rt_str_to_upper", ffi::subscript_rt_str_to_upper as *const u8),
        ("subscript_rt_str_to_lower", ffi::subscript_rt_str_to_lower as *const u8),
        ("subscript_rt_str_replace", ffi::subscript_rt_str_replace as *const u8),
        (
            "subscript_rt_str_replace_all",
            ffi::subscript_rt_str_replace_all as *const u8,
        ),
        (
            "subscript_rt_str_substring",
            ffi::subscript_rt_str_substring as *const u8,
        ),
        ("subscript_rt_str_substr", ffi::subscript_rt_str_substr as *const u8),
        ("subscript_rt_str_char_at", ffi::subscript_rt_str_char_at as *const u8),
        (
            "subscript_rt_str_code_point_at",
            ffi::subscript_rt_str_code_point_at as *const u8,
        ),
        (
            "subscript_rt_str_method_concat",
            ffi::subscript_rt_str_method_concat as *const u8,
        ),
        // Array method intrinsics (stdlib.md §9): one opaque symbol
        // per accepted method, ArrFn::ALL order.
        ("subscript_rt_arr_index_of", ffi::subscript_rt_arr_index_of as *const u8),
        (
            "subscript_rt_arr_last_index_of",
            ffi::subscript_rt_arr_last_index_of as *const u8,
        ),
        ("subscript_rt_arr_includes", ffi::subscript_rt_arr_includes as *const u8),
        ("subscript_rt_arr_join", ffi::subscript_rt_arr_join as *const u8),
        ("subscript_rt_arr_slice", ffi::subscript_rt_arr_slice as *const u8),
        ("subscript_rt_arr_fill", ffi::subscript_rt_arr_fill as *const u8),
        ("subscript_rt_arr_reverse", ffi::subscript_rt_arr_reverse as *const u8),
        ("subscript_rt_arr_concat", ffi::subscript_rt_arr_concat as *const u8),
        ("subscript_rt_arr_for_each", ffi::subscript_rt_arr_for_each as *const u8),
        ("subscript_rt_arr_map", ffi::subscript_rt_arr_map as *const u8),
        ("subscript_rt_arr_filter", ffi::subscript_rt_arr_filter as *const u8),
        ("subscript_rt_arr_reduce", ffi::subscript_rt_arr_reduce as *const u8),
        ("subscript_rt_arr_some", ffi::subscript_rt_arr_some as *const u8),
        ("subscript_rt_arr_every", ffi::subscript_rt_arr_every as *const u8),
        (
            "subscript_rt_arr_find_index",
            ffi::subscript_rt_arr_find_index as *const u8,
        ),
        ("subscript_rt_arr_sort", ffi::subscript_rt_arr_sort as *const u8),
        (
            "subscript_rt_arr_reduce_right",
            ffi::subscript_rt_arr_reduce_right as *const u8,
        ),
        ("subscript_rt_arr_splice", ffi::subscript_rt_arr_splice as *const u8),
        ("subscript_rt_arr_shift", ffi::subscript_rt_arr_shift as *const u8),
        ("subscript_rt_arr_unshift", ffi::subscript_rt_arr_unshift as *const u8),
        (
            "subscript_rt_arr_copy_within",
            ffi::subscript_rt_arr_copy_within as *const u8,
        ),
        (
            "subscript_rt_fixed_arr_for_each",
            ffi::subscript_rt_fixed_arr_for_each as *const u8,
        ),
        ("subscript_rt_fixed_arr_map", ffi::subscript_rt_fixed_arr_map as *const u8),
        (
            "subscript_rt_fixed_arr_filter",
            ffi::subscript_rt_fixed_arr_filter as *const u8,
        ),
        (
            "subscript_rt_fixed_arr_reduce",
            ffi::subscript_rt_fixed_arr_reduce as *const u8,
        ),
        ("subscript_rt_fixed_arr_some", ffi::subscript_rt_fixed_arr_some as *const u8),
        (
            "subscript_rt_fixed_arr_every",
            ffi::subscript_rt_fixed_arr_every as *const u8,
        ),
        (
            "subscript_rt_fixed_arr_find_index",
            ffi::subscript_rt_fixed_arr_find_index as *const u8,
        ),
        (
            "subscript_rt_fixed_arr_reduce_right",
            ffi::subscript_rt_fixed_arr_reduce_right as *const u8,
        ),
        // Map/Set intrinsics (stdlib.md §10, Q24).
        ("subscript_rt_map_new", ffi::subscript_rt_map_new as *const u8),
        ("subscript_rt_map_size", ffi::subscript_rt_map_size as *const u8),
        ("subscript_rt_map_get", ffi::subscript_rt_map_get as *const u8),
        ("subscript_rt_map_get_or", ffi::subscript_rt_map_get_or as *const u8),
        ("subscript_rt_map_set", ffi::subscript_rt_map_set as *const u8),
        ("subscript_rt_map_has", ffi::subscript_rt_map_has as *const u8),
        ("subscript_rt_map_delete", ffi::subscript_rt_map_delete as *const u8),
        ("subscript_rt_map_clear", ffi::subscript_rt_map_clear as *const u8),
        (
            "subscript_rt_map_for_each",
            ffi::subscript_rt_map_for_each as *const u8,
        ),
        (
            "subscript_rt_map_group_by",
            ffi::subscript_rt_map_group_by as *const u8,
        ),
        ("subscript_rt_set_new", ffi::subscript_rt_set_new as *const u8),
        ("subscript_rt_set_size", ffi::subscript_rt_set_size as *const u8),
        ("subscript_rt_set_add", ffi::subscript_rt_set_add as *const u8),
        ("subscript_rt_set_has", ffi::subscript_rt_set_has as *const u8),
        ("subscript_rt_set_delete", ffi::subscript_rt_set_delete as *const u8),
        ("subscript_rt_set_clear", ffi::subscript_rt_set_clear as *const u8),
        (
            "subscript_rt_set_for_each",
            ffi::subscript_rt_set_for_each as *const u8,
        ),
        ("subscript_rt_set_union", ffi::subscript_rt_set_union as *const u8),
        (
            "subscript_rt_set_intersection",
            ffi::subscript_rt_set_intersection as *const u8,
        ),
        (
            "subscript_rt_set_difference",
            ffi::subscript_rt_set_difference as *const u8,
        ),
        (
            "subscript_rt_set_symmetric_difference",
            ffi::subscript_rt_set_symmetric_difference as *const u8,
        ),
        (
            "subscript_rt_set_is_subset_of",
            ffi::subscript_rt_set_is_subset_of as *const u8,
        ),
        (
            "subscript_rt_set_is_superset_of",
            ffi::subscript_rt_set_is_superset_of as *const u8,
        ),
        (
            "subscript_rt_set_is_disjoint_from",
            ffi::subscript_rt_set_is_disjoint_from as *const u8,
        ),
        ("subscript_rt_date_utc", ffi::subscript_rt_date_utc as *const u8),
        ("subscript_rt_date_new", ffi::subscript_rt_date_new as *const u8),
        ("subscript_rt_date_now", ffi::subscript_rt_date_now as *const u8),
        ("subscript_rt_date_get", ffi::subscript_rt_date_get as *const u8),
        ("subscript_rt_date_to_iso", ffi::subscript_rt_date_to_iso as *const u8),
    ];
    for (name, addr) in syms {
        builder.symbol(*name, *addr);
    }
    let regex_symbols: &[(&str, *const u8)] = &[
        ("subscript_rt_regex_new", ffi::subscript_rt_regex_new as *const u8),
        ("subscript_rt_regex_test", ffi::subscript_rt_regex_test as *const u8),
        ("subscript_rt_regex_source", ffi::subscript_rt_regex_source as *const u8),
        ("subscript_rt_regex_flags", ffi::subscript_rt_regex_flags as *const u8),
        ("subscript_rt_regex_search", ffi::subscript_rt_regex_search as *const u8),
        (
            "subscript_rt_regex_replace",
            ffi::subscript_rt_regex_replace as *const u8,
        ),
        (
            "subscript_rt_regex_replace_all",
            ffi::subscript_rt_regex_replace_all as *const u8,
        ),
        ("subscript_rt_regex_split", ffi::subscript_rt_regex_split as *const u8),
        (
            "subscript_rt_regex_match_start",
            ffi::subscript_rt_regex_match_start as *const u8,
        ),
        (
            "subscript_rt_regex_match_end",
            ffi::subscript_rt_regex_match_end as *const u8,
        ),
    ];
    for (name, address) in regex_symbols {
        builder.symbol(*name, *address);
    }
}

/// Checks `files`, lowers the typed HIR through the shared CLIF
/// lowering, and finalizes the code in a live JIT module.
fn compile_jit(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<(JITModule, Lowered), RunError> {
    let hir = check_program(files).map_err(RunError::Rejected)?;

    let flags = dev_flags().map_err(RunError::Internal)?;
    let isa = cranelift_native::builder()
        .map_err(|e| RunError::Internal(internal(format!("host ISA: {e}"))))
        .and_then(|b| {
            b.finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))
        })?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    register_runtime(&mut builder);
    register_symbols(&mut builder, libraries);
    let mut module = JITModule::new(builder);

    let lowered = lower_module_with(&mut module, &hir, LowerOptions::default())
        .map_err(RunError::Internal)?;
    if let Some(name) = missing_symbol(&lowered.foreign_symbols, libraries) {
        // Cranelift-JIT retains a platform symbol-lookup fallback and
        // exposes only an API for appending more lookup functions. The
        // lowering's list is total: its sole `Callee::Foreign` import path
        // records every imported name. Refuse finalization here so the
        // fallback is never consulted for an unregistered foreign symbol.
        // SAFETY: no definition was finalized or executed and no pointer
        // into this module escaped.
        unsafe { module.free_memory() };
        return Err(RunError::UnresolvedForeignSymbol(name.to_string()));
    }
    module
        .finalize_definitions()
        .map_err(|e| RunError::Internal(internal(format!("finalize: {e}"))))?;
    Ok((module, lowered))
}

/// Calls one finalized `(subscript_rt_context*) -> void` script entry under the host
/// depth discipline.
///
/// # Safety
///
/// `entry` must be finalized generated code of this exact signature and
/// remain live for the call.
unsafe fn call_script_entry(entry: *const u8, ctx: &mut Context) {
    type Entry = unsafe extern "C" fn(*mut Context);
    // SAFETY: guaranteed by the caller.
    let entry: Entry = unsafe { std::mem::transmute(entry) };
    ctx.enter_script();
    // SAFETY: generated code never unwinds across this C boundary.
    unsafe { entry(ctx) };
    ctx.exit_script();
}

struct CompletedRun {
    ctx: Box<Context>,
    stdout: Vec<u8>,
    elapsed: Duration,
}

/// Runs the module initializer and then the exported `main` on a fresh
/// Context, returning the stdout bytes of the run and how long the
/// `main` call itself took.
///
/// The initializer is deliberately outside the measured span: it is the
/// module's global setup, not the workload (`specs/blocks/compiler.md`
/// §9). Running it before every call also restores the module globals,
/// so repeated calls of the same `main` are the same computation.
fn execute_entry(
    module: &JITModule,
    lowered: &Lowered,
    fail_alloc_after: Option<u64>,
    freed_handle_diagnostics: bool,
) -> Result<CompletedRun, RunError> {
    let init_ptr = module.get_finalized_function(lowered.init);
    let main_ptr = module.get_finalized_function(lowered.main);

    let mut ctx = Context::new();
    let diagnostics_set = ctx.set_freed_handle_diagnostics(
        freed_handle_diagnostics,
        0,
        FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES,
    );
    debug_assert!(
        diagnostics_set,
        "dev-JIT freed-handle diagnostics setting was attempted after Context allocation started"
    );
    if let Some(n) = fail_alloc_after {
        ctx.fail_alloc_after(n);
    }
    let mut elapsed = Duration::ZERO;
    {
        // SAFETY: `init_ptr`/`main_ptr` are finalized JIT code for
        // functions the lowering built with exactly this signature
        // (`(ctx) -> void`, host C calling convention); the module
        // outlives both calls; `ctx` is a live exclusive Context.
        // Generated code never unwinds (traps return through the
        // flag-check paths), so no panic crosses this boundary.
        unsafe {
            call_script_entry(init_ptr, &mut ctx);
            if !ctx.trapped() {
                type Entry = unsafe extern "C" fn(*mut Context);
                let main: Entry = std::mem::transmute(main_ptr);
                ctx.enter_script();
                let start = Instant::now();
                main(&mut *ctx);
                elapsed = start.elapsed();
                ctx.exit_script();
            }
            if !ctx.trapped() {
                for entry in &lowered.entries {
                    if entry.is_async && entry.name != "main" {
                        let entry_ptr = module.get_finalized_function(entry.id);
                        call_script_entry(entry_ptr, &mut ctx);
                        if ctx.trapped() {
                            break;
                        }
                    }
                }
            }
            while !ctx.trapped() && ctx.async_pending() != 0 {
                // SAFETY: every pending item was registered by a generated
                // async export wrapper in this finalized module.
                ctx.async_step();
            }
        }
    }

    let trap = ctx.trap_record().map(|r| {
        let pos = lowered
            .positions
            .get(r.pos_id as usize)
            .cloned()
            .unwrap_or_else(|| Pos::new(String::new(), 0, 0));
        (r.kind, r.message.clone(), pos)
    });
    let stdout = ctx.take_stdout();
    match trap {
        Some((rule, message, pos)) => Err(RunError::Trap(TrapReport {
            rule,
            message,
            pos,
            stdout,
        })),
        None => Ok(CompletedRun {
            ctx,
            stdout,
            elapsed,
        }),
    }
}

fn run_entry(
    module: &JITModule,
    lowered: &Lowered,
    fail_alloc_after: Option<u64>,
) -> Result<(Vec<u8>, Duration), RunError> {
    execute_entry(module, lowered, fail_alloc_after, false)
        .map(|run| (run.stdout, run.elapsed))
}

fn memory_accounting(ctx: &Context) -> JitMemoryAccounting {
    let p: *const Context = ctx;
    // SAFETY: shared host accessors over a live Context after every script
    // entry returned.
    unsafe {
        JitMemoryAccounting {
            live_bytes: ffi::subscript_rt_ctx_live_bytes(p),
            reserved_bytes: ffi::subscript_rt_ctx_reserved_bytes(p),
        }
    }
}

/// Checks `files`, lowers the typed HIR through the shared CLIF
/// lowering, executes the exported `main(): void` under the dev JIT,
/// and returns the exact stdout bytes the run produced.
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when the run trapped (rule + message + TS
/// position + pre-trap stdout),
/// [`RunError::UnresolvedForeignSymbol`] when the program calls a symbol
/// but no native library was supplied, and [`RunError::Internal`] on
/// backend failures.
pub fn run_jit(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    run_jit_with_native_libraries(files, &[])
}

/// Runs `files` through the dev JIT and returns its exact stdout bytes
/// together with Context memory accounting observed after `main` returns.
///
/// The accounting is read while the fresh run's Context is still alive and
/// before its allocations are released. `freed_handle_diagnostics`
/// establishes §8.1a-3's retain-and-poison mode with threshold 0 and an
/// 1 GiB recommended default budget before `subscript_init`; when false, the dev
/// tier's default immediate-release policy applies.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit`].
pub fn run_jit_with_memory_accounting(
    files: &[SourceFile],
    freed_handle_diagnostics: bool,
) -> Result<(Vec<u8>, JitMemoryAccounting), RunError> {
    let (module, lowered) = compile_jit(files, &[])?;
    let outcome = execute_entry(&module, &lowered, None, freed_handle_diagnostics)
        .map(|run| (run.stdout, memory_accounting(&run.ctx)));
    // SAFETY: all executions above have returned and no pointer into
    // the JIT-allocated code/data survives (the Context held none).
    unsafe { module.free_memory() };
    outcome
}

/// Checks, lowers, and runs `files` through the dev JIT with the
/// caller-supplied native libraries available for foreign calls.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit`], including
/// [`RunError::UnresolvedForeignSymbol`] when a called foreign symbol is
/// absent from `libraries`.
pub fn run_jit_with_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    let (module, lowered) = compile_jit(files, libraries)?;
    let outcome = run_entry(&module, &lowered, None).map(|(out, _)| out);
    // SAFETY: all executions above have returned and no pointer into
    // the JIT-allocated code/data survives (the Context held none).
    unsafe { module.free_memory() };
    outcome
}

/// Runs the dev JIT with freed-handle diagnostics enabled and the
/// caller-supplied native libraries available for foreign calls.
///
/// The mode uses threshold 0 and the recommended 1 GiB retention budget,
/// matching [`run_jit_with_memory_accounting`] when its diagnostics argument
/// is true.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit_with_native_libraries`].
pub fn run_jit_with_freed_handle_diagnostics_and_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    let (module, lowered) = compile_jit(files, libraries)?;
    let outcome = execute_entry(&module, &lowered, None, true).map(|run| run.stdout);
    // SAFETY: all executions above have returned and no pointer into
    // the JIT-allocated code/data survives (the Context held none).
    unsafe { module.free_memory() };
    outcome
}

/// Runs the dev tier while refusing the `n`-th object-level Context
/// allocation after Context creation.
///
/// The injected fault is armed before `subscript_init`, so module-initializer
/// allocations are part of the count.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit`].
pub fn run_jit_with_alloc_failure(
    files: &[SourceFile],
    n: u64,
) -> Result<Vec<u8>, RunError> {
    let (module, lowered) = compile_jit(files, &[])?;
    let outcome = run_entry(&module, &lowered, Some(n)).map(|(out, _)| out);
    // SAFETY: all executions above have returned and no pointer into
    // the JIT-allocated code/data survives (the Context held none).
    unsafe { module.free_memory() };
    outcome
}

/// Timed samples for one subject of the P4 performance gate
/// (`specs/blocks/compiler.md` §9).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BenchSamples {
    /// Exact stdout bytes the workload produced. Every run of a
    /// subject produces these same bytes, so they can be compared
    /// against the entry's golden.
    pub stdout: Vec<u8>,
    /// Elapsed time of each timed run, in order; warm-up runs are not
    /// included.
    pub samples: Vec<Duration>,
    /// Sum of the measured workload-call durations discarded as warm-up.
    pub warmup: Duration,
    /// Number of workload calls discarded as warm-up.
    pub warmup_iterations: usize,
    /// Time spent turning source into executable code before the first
    /// run (reported, never gated — §9).
    pub compile: Duration,
}

/// Measures the dev-JIT tier on `files`: compiles once, then calls the
/// exported `main(): void` `warmup + timed` times, timing each call
/// and keeping the last `timed` samples.
///
/// The measured span is the `main` call alone — compilation, module
/// finalization, Context creation, and the global initializer are all
/// outside it (§9). Every run must produce identical stdout bytes; a
/// difference is an internal error, because a workload whose result
/// changes between runs is not the workload that was verified against
/// the golden.
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when a run trapped, [`RunError::Internal`] when
/// `timed` is zero, on backend failures, or when two runs disagreed.
pub fn jit_bench(
    files: &[SourceFile],
    warmup: usize,
    timed: usize,
) -> Result<BenchSamples, RunError> {
    jit_bench_with_warmup_floor(files, warmup, timed, Duration::ZERO)
}

/// Measures the dev-JIT tier like [`jit_bench`], but continues warm-up until
/// both `warmup` calls and `warmup_floor` of measured workload execution have
/// completed.
///
/// The returned [`BenchSamples::warmup`] is the sum of the workload-call
/// durations only. Compilation, Context construction, initialization, and I/O
/// remain outside both the warm-up and timed spans.
///
/// # Errors
///
/// Returns the same errors as [`jit_bench`].
pub fn jit_bench_with_warmup_floor(
    files: &[SourceFile],
    warmup: usize,
    timed: usize,
    warmup_floor: Duration,
) -> Result<BenchSamples, RunError> {
    if timed == 0 {
        return Err(RunError::Internal(internal(
            "a benchmark subject needs at least one timed run",
        )));
    }
    let started = Instant::now();
    let (module, lowered) = compile_jit(files, &[])?;
    let compile = started.elapsed();

    let mut samples = Vec::with_capacity(timed);
    let mut warmup_elapsed = Duration::ZERO;
    let mut warmup_iterations = 0;
    let mut stdout: Option<Vec<u8>> = None;
    let mut failure: Option<RunError> = None;
    while warmup_iterations < warmup || warmup_elapsed < warmup_floor {
        match run_entry(&module, &lowered, None) {
            Ok((out, elapsed)) => {
                match &stdout {
                    Some(first) if first != &out => {
                        failure = Some(RunError::Internal(internal(
                            "the dev-JIT workload produced different output on two runs",
                        )));
                    }
                    Some(_) => {}
                    None => stdout = Some(out),
                }
                if failure.is_some() {
                    break;
                }
                warmup_elapsed += elapsed;
                warmup_iterations += 1;
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    if failure.is_none() {
        for _ in 0..timed {
            match run_entry(&module, &lowered, None) {
                Ok((out, elapsed)) => {
                    match &stdout {
                        Some(first) if first != &out => {
                            failure = Some(RunError::Internal(internal(
                                "the dev-JIT workload produced different output on two runs",
                            )));
                        }
                        Some(_) => {}
                        None => stdout = Some(out),
                    }
                    if failure.is_some() {
                        break;
                    }
                    samples.push(elapsed);
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
    }
    // SAFETY: all executions above have returned and no pointer into
    // the JIT-allocated code/data survives (the Context held none).
    unsafe { module.free_memory() };

    if let Some(e) = failure {
        return Err(e);
    }
    Ok(BenchSamples {
        stdout: stdout.unwrap_or_default(),
        samples,
        warmup: warmup_elapsed,
        warmup_iterations,
        compile,
    })
}

#[cfg(test)]
pub(crate) fn memory_accounting_after_run(
    files: &[SourceFile],
) -> Result<(u64, u64, u64), RunError> {
    let (module, lowered) = compile_jit(files, &[])?;
    let result = execute_entry(&module, &lowered, None, false).map(|run| {
        let p: *const Context = &*run.ctx;
        let accounting = memory_accounting(&run.ctx);
        // SAFETY: shared host accessors over a live Context after every
        // script entry returned.
        unsafe {
            (
                ffi::subscript_rt_ctx_live_allocations(p),
                accounting.live_bytes,
                accounting.reserved_bytes,
            )
        }
    });
    // SAFETY: all entries returned and no code pointer survives.
    unsafe { module.free_memory() };
    result
}

#[cfg(test)]
type AllocationAttribution = (Vec<(u32, u32, u64)>, Vec<Pos>);

#[cfg(test)]
pub(crate) fn allocation_attribution_after_run(
    files: &[SourceFile],
) -> Result<AllocationAttribution, RunError> {
    unsafe extern "C" fn collect(
        userdata: *mut std::ffi::c_void,
        class_id: u32,
        pos_id: u32,
        payload_bytes: u64,
    ) {
        // SAFETY: this helper passes a live Vec of this exact type.
        let triples = unsafe {
            &mut *userdata.cast::<Vec<(u32, u32, u64)>>()
        };
        triples.push((class_id, pos_id, payload_bytes));
    }

    let (module, lowered) = compile_jit(files, &[])?;
    let init = module.get_finalized_function(lowered.init);
    let main = module.get_finalized_function(lowered.main);
    let mut ctx = Context::new();
    // SAFETY: both pointers are finalized entries and the module remains
    // live through the calls.
    unsafe {
        call_script_entry(init, &mut ctx);
        if !ctx.trapped() {
            call_script_entry(main, &mut ctx);
        }
    }
    let result = match ctx.trap_record() {
        Some(record) => Err(RunError::Internal(internal(format!(
            "attribution probe trapped: {}",
            record.message
        )))),
        None => {
            let mut triples = Vec::new();
            let p: *const Context = &*ctx;
            // SAFETY: shared host inspection after every script entry
            // returned; callback userdata is a live Vec.
            let visited = unsafe {
                ffi::subscript_rt_ctx_visit_live_allocations(
                    p,
                    Some(collect),
                    (&mut triples as *mut Vec<(u32, u32, u64)>).cast(),
                )
            };
            if visited as usize != triples.len() {
                Err(RunError::Internal(internal(
                    "allocation visitor count differs from callbacks",
                )))
            } else {
                triples.sort_unstable();
                Ok((triples, lowered.positions.clone()))
            }
        }
    };
    // SAFETY: all entries and callbacks returned; no code pointer survives.
    unsafe { module.free_memory() };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ObservedTrap {
        calls: u32,
        kind: u32,
        pos_id: u32,
        message: Vec<u8>,
    }

    impl Default for ObservedTrap {
        fn default() -> Self {
            Self {
                calls: 0,
                kind: 0,
                pos_id: 0,
                message: Vec::new(),
            }
        }
    }

    unsafe extern "C" fn observe_trap(
        userdata: *mut std::ffi::c_void,
        kind: u32,
        pos_id: u32,
        message: *const u8,
        message_len: u64,
    ) {
        // SAFETY: each test passes a live `ObservedTrap` as userdata.
        let observed = unsafe { &mut *userdata.cast::<ObservedTrap>() };
        observed.calls += 1;
        observed.kind = kind;
        observed.pos_id = pos_id;
        // SAFETY: the trap observer supplies the stored record's bytes.
        observed.message =
            unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
    }

    fn sources(src: &str) -> Vec<SourceFile> {
        vec![SourceFile::new("test.ts", src)]
    }

    unsafe fn call_entry(code: *const u8, ctx: &mut Context) {
        type Entry = unsafe extern "C" fn(*mut Context);
        // SAFETY: callers pass a finalized `(ctx) -> void` entry and
        // keep its JIT module alive for the duration of this call.
        let entry: Entry = unsafe { std::mem::transmute(code) };
        ctx.enter_script();
        // SAFETY: finalized generated code never unwinds across FFI.
        unsafe { entry(ctx) };
        ctx.exit_script();
    }

    #[test]
    fn fused_for_of_over_populated_containers_and_bmp_allocates_nothing() {
        let program = sources(
            "const values: i32[] = [1, 2, 3];\n\
             const text: string = \"Aé漢\";\n\
             const map: Map<i32, i32> = new Map<i32, i32>();\n\
             let sink: i32 = 0;\n\
             export function populate(): void {\n\
               map.set(4, 40);\n\
               map.set(5, 50);\n\
             }\n\
             export function iterate(): void {\n\
               for (const value of values) { sink += value; }\n\
               for (const value of map.values()) { sink += value; }\n\
               for (const codePoint of text) { sink += codePoint.length; }\n\
             }\n\
             export function main(): void {}\n",
        );
        let (module, lowered) =
            compile_jit(&program, &[]).expect("compile allocation probe");
        let init = module.get_finalized_function(lowered.init);
        let populate = lowered
            .entries
            .iter()
            .find(|entry| entry.name == "populate")
            .map(|entry| module.get_finalized_function(entry.id))
            .expect("populate entry");
        let iterate = lowered
            .entries
            .iter()
            .find(|entry| entry.name == "iterate")
            .map(|entry| module.get_finalized_function(entry.id))
            .expect("iterate entry");
        let mut ctx = Context::new();
        let p: *const Context = &*ctx;
        // SAFETY: all three entries are finalized and the module stays
        // alive through the calls.
        unsafe {
            call_entry(init, &mut ctx);
            call_entry(populate, &mut ctx);
        }
        assert!(!ctx.trapped(), "probe setup trapped: {:?}", ctx.trap_record());
        // SAFETY: shared access after the setup entry returned.
        let before = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        // SAFETY: finalized allocation-probe entry.
        unsafe { call_entry(iterate, &mut ctx) };
        assert!(!ctx.trapped(), "fused loop trapped: {:?}", ctx.trap_record());
        // SAFETY: shared access after the iteration entry returned.
        let after = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        assert_eq!(
            after, before,
            "array/Map/BMP-string for…of introduced a live Context allocation"
        );

        // SAFETY: every generated entry returned and no code pointer
        // survives the test.
        unsafe { module.free_memory() };
    }

    #[test]
    fn jit_host_trap_observer_and_clear_api_preserve_unwind_semantics() {
        let program = sources(
            "let calls: i32 = 0;\n\
             export function main(): void {\n\
               calls += 1;\n\
               print(`start:${calls}`);\n\
               if (calls === 1) {\n\
                 const failed: JsonResult<i32> = JSON.parse<i32>(\"nope\");\n\
                 print(`${failed.value}`);\n\
               }\n\
               print(\"done\");\n\
             }\n",
        );
        let (module, lowered) =
            compile_jit(&program, &[]).expect("compile observer program");
        let init = module.get_finalized_function(lowered.init);
        let main = module.get_finalized_function(lowered.main);
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let mut observed = ObservedTrap::default();
        // SAFETY: live Context and observer userdata; the callback does
        // not receive or recover the Context.
        unsafe {
            ffi::subscript_rt_ctx_set_trap_observer(
                p,
                Some(observe_trap),
                (&mut observed as *mut ObservedTrap).cast(),
            );
            call_entry(init, &mut ctx);
            call_entry(main, &mut ctx);
        }

        assert_eq!(observed.calls, 1);
        // SAFETY: shared access to the live Context after script return.
        assert_eq!(observed.kind, unsafe { ffi::subscript_rt_ctx_trap_kind(p) });
        // SAFETY: shared access to the live Context after script return.
        assert_eq!(observed.pos_id, unsafe { ffi::subscript_rt_ctx_trap_pos_id(p) });
        let mut message_len = 0;
        // SAFETY: shared live Context and writable length.
        let message = unsafe { ffi::subscript_rt_ctx_trap_message(p, &mut message_len) };
        // SAFETY: the accessor returns `message_len` record-owned bytes.
        let message = unsafe { std::slice::from_raw_parts(message, message_len as usize) };
        assert_eq!(observed.message, message);
        assert!(ctx.trapped(), "observer must not suppress the trap");
        assert_eq!(
            ctx.stdout_bytes(),
            b"start:1\n",
            "the first call must unwind before the trailing print"
        );

        // SAFETY: the first call has fully returned to the host boundary.
        assert_eq!(unsafe { ffi::subscript_rt_ctx_clear_trap(p) }, 1);
        // SAFETY: same finalized entry, now on a clear Context.
        unsafe { call_entry(main, &mut ctx) };
        assert!(!ctx.trapped());
        assert_eq!(
            ctx.stdout_bytes(),
            b"start:1\nstart:2\ndone\n",
            "the second call must execute beyond the stale trap check"
        );
        assert_eq!(observed.calls, 1, "the successful call must not notify");

        // Re-run initialization on a fresh Context so `main` traps
        // again, but clear the observer first through the null ABI form.
        let mut cleared_ctx = Context::new();
        let cleared_p: *mut Context = &mut *cleared_ctx;
        let mut cleared_observed = ObservedTrap::default();
        // SAFETY: live Context and userdata; null explicitly unregisters.
        unsafe {
            ffi::subscript_rt_ctx_set_trap_observer(
                cleared_p,
                Some(observe_trap),
                (&mut cleared_observed as *mut ObservedTrap).cast(),
            );
            ffi::subscript_rt_ctx_set_trap_observer(cleared_p, None, std::ptr::null_mut());
            call_entry(init, &mut cleared_ctx);
            call_entry(main, &mut cleared_ctx);
        }
        assert!(cleared_ctx.trapped());
        assert_eq!(cleared_observed.calls, 0);

        // SAFETY: all calls returned and neither Context retains JIT
        // code pointers.
        unsafe { module.free_memory() };
    }

    #[test]
    fn jit_corpus_output_is_byte_identical_with_an_observer_registered() {
        let source = include_str!("../../corpus/accept/a01-hello.ts");
        let program = [SourceFile::new("a01-hello.ts", source)];
        let (module, lowered) = compile_jit(&program, &[]).expect("compile a01");
        let init = module.get_finalized_function(lowered.init);
        let main = module.get_finalized_function(lowered.main);

        let run = |with_observer: bool| {
            let mut ctx = Context::new();
            let mut observed = ObservedTrap::default();
            if with_observer {
                // SAFETY: live Context and callback userdata.
                unsafe {
                    ffi::subscript_rt_ctx_set_trap_observer(
                        &mut *ctx,
                        Some(observe_trap),
                        (&mut observed as *mut ObservedTrap).cast(),
                    );
                }
            }
            // SAFETY: finalized entries; module remains alive.
            unsafe {
                call_entry(init, &mut ctx);
                call_entry(main, &mut ctx);
            }
            assert!(!ctx.trapped());
            assert_eq!(observed.calls, 0);
            ctx.take_stdout()
        };

        let without = run(false);
        let with = run(true);
        assert_eq!(with, without, "observer changed a01 stdout bytes");
        assert_eq!(with, b"hello\n");

        // SAFETY: every execution returned and no code pointer survives.
        unsafe { module.free_memory() };
    }

    #[test]
    fn jit_bench_keeps_one_sample_per_timed_run() {
        let b = jit_bench(
            &sources("export function main(): void {\n  print(\"tick\");\n}\n"),
            2,
            3,
        )
        .expect("bench run");
        assert_eq!(b.stdout, b"tick\n");
        assert_eq!(b.samples.len(), 3);
        assert_eq!(b.warmup_iterations, 2);
        assert!(b.warmup > Duration::ZERO);
        assert!(b.compile > Duration::ZERO);
    }

    #[test]
    fn jit_bench_reruns_the_initializer_so_globals_are_restored() {
        // `counter` is a module global: without a fresh initializer
        // per run the second run would print 12.
        let b = jit_bench(
            &sources(
                "let counter: i32 = 10;\nexport function main(): void {\n  counter += 1;\n  print(`${counter}`);\n}\n",
            ),
            0,
            4,
        )
        .expect("bench run");
        assert_eq!(b.stdout, b"11\n");
        assert_eq!(b.samples.len(), 4);
    }

    #[test]
    fn jit_memory_accounting_distinguishes_retention_from_live_growth() {
        let measure = |count: i32, free: bool| {
            let statement = if free {
                "const value: Cell = new Cell(i, null);\n\
                 Context.free(value);"
            } else {
                "kept = new Cell(i, kept);"
            };
            let source = format!(
                "class Cell {{\n\
                   value: i32;\n\
                   next: Cell | null;\n\
                   constructor(value: i32, next: Cell | null) {{\n\
                     this.value = value;\n\
                     this.next = next;\n\
                   }}\n\
                 }}\n\
                 let kept: Cell | null = null;\n\
                 export function main(): void {{\n\
                   for (let i: i32 = 0; i < {count}; i += 1) {{\n\
                     {statement}\n\
                   }}\n\
                 }}\n"
            );
            let (stdout, accounting) = run_jit_with_memory_accounting(
                &[SourceFile::new("accounting.ts", source)],
                true,
            )
            .expect("accounted dev-JIT run");
            assert!(stdout.is_empty());
            accounting
        };

        let freed_small = measure(8, true);
        let freed_large = measure(64, true);
        assert_eq!(
            freed_small.live_bytes, freed_large.live_bytes,
            "freeing every allocation must keep the live payload flat"
        );
        assert!(
            freed_large.reserved_bytes > freed_small.reserved_bytes,
            "retained layouts must make reserved bytes grow with allocation count"
        );

        let kept_small = measure(8, false);
        let kept_large = measure(64, false);
        assert!(
            kept_large.live_bytes > kept_small.live_bytes,
            "keeping allocations must make live payload bytes grow"
        );
        assert!(
            kept_large.reserved_bytes > kept_small.reserved_bytes,
            "keeping allocations must make reserved bytes grow"
        );
    }

    #[test]
    fn jit_bench_needs_a_timed_run() {
        let err = jit_bench(&sources("export function main(): void {}\n"), 1, 0);
        assert!(matches!(err, Err(RunError::Internal(_))));
    }

    #[test]
    fn date_now_reads_the_pinned_context_clock_in_the_dev_tier() {
        // stdlib.md §3: `Date.now()` is Context-owned and pinnable. The
        // public `run_jit` builds its own Context, so this drives the
        // compiled entries directly on a Context whose clock is pinned.
        // Both tiers call the identical `subscript_rt_date_now` symbol; the
        // ship tier's link resolves it from the same runtime. The
        // ship-tier half of the both-tier check is
        // `tests/cemit.rs::date_now_reads_the_pinned_context_clock_in_the_ship_tier`
        // — the same program, pinned ms, and expected bytes.
        let (module, lowered) = compile_jit(&sources(
            "export function main(): void {\n  const t: i64 = Date.now();\n  print(`${t}`);\n  print(new Date(Date.now()).toISOString());\n}\n",
        ), &[])
        .expect("compile");
        let init_ptr = module.get_finalized_function(lowered.init);
        let main_ptr = module.get_finalized_function(lowered.main);
        let mut ctx = Context::new();
        ctx.set_now(1_592_224_496_789);
        type Entry = unsafe extern "C" fn(*mut Context);
        // SAFETY: finalized JIT code with the `(ctx) -> void` entry
        // signature; the module outlives the calls; generated code
        // never unwinds (trap-flag discipline).
        unsafe {
            let init: Entry = std::mem::transmute(init_ptr);
            init(&mut *ctx);
            let main: Entry = std::mem::transmute(main_ptr);
            main(&mut *ctx);
        }
        assert!(ctx.trap_record().is_none());
        assert_eq!(
            ctx.take_stdout(),
            b"1592224496789\n2020-06-15T12:34:56.789Z\n"
        );
        // SAFETY: all executions above have returned; no pointer into
        // the JIT memory survives.
        unsafe { module.free_memory() };
    }

    #[test]
    fn jit_bench_surfaces_a_trap() {
        let err = jit_bench(
            &sources("export function main(): void {\n  const xs: i32[] = [];\n  xs.pop();\n}\n"),
            0,
            1,
        );
        assert!(matches!(err, Err(RunError::Trap(_))));
    }
}
