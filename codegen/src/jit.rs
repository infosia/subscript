//! The dev-tier JIT driver: instantiates the tier-neutral lowering
//! with `cranelift-jit`, resolves the runtime's `extern "C"` symbols,
//! runs the exported `main(): void`, and returns the captured stdout
//! bytes or a trap report.

use std::time::{Duration, Instant};

use cranelift_jit::{JITBuilder, JITModule};
use subscript_compiler::{check_program, Diagnostic, Pos, SourceFile};
use subscript_runtime::{ffi, Context, TrapKind};

use crate::lower::{dev_flags, internal, lower_module_with, Lowered, LowerOptions};

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
}

impl std::fmt::Display for TrapReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: trap [{}]: {}", self.pos, self.rule, self.message)
    }
}

/// Why a run produced no output.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunError {
    /// The program was rejected by the checker (P1 diagnostics).
    Rejected(Vec<Diagnostic>),
    /// The program ran and trapped.
    Trap(TrapReport),
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
        ("sub_rt_print", ffi::sub_rt_print as *const u8),
        ("sub_rt_collect", ffi::sub_rt_collect as *const u8),
        ("sub_rt_alloc", ffi::sub_rt_alloc as *const u8),
        ("sub_rt_delete", ffi::sub_rt_delete as *const u8),
        ("sub_rt_trap", ffi::sub_rt_trap as *const u8),
        ("sub_rt_root_add", ffi::sub_rt_root_add as *const u8),
        ("sub_rt_shadow_push", ffi::sub_rt_shadow_push as *const u8),
        ("sub_rt_shadow_pop", ffi::sub_rt_shadow_pop as *const u8),
        ("sub_rt_str_lit", ffi::sub_rt_str_lit as *const u8),
        ("sub_rt_str_len", ffi::sub_rt_str_len as *const u8),
        ("sub_rt_str_concat", ffi::sub_rt_str_concat as *const u8),
        ("sub_rt_str_slice", ffi::sub_rt_str_slice as *const u8),
        ("sub_rt_str_eq", ffi::sub_rt_str_eq as *const u8),
        ("sub_rt_fmt_i32", ffi::sub_rt_fmt_i32 as *const u8),
        ("sub_rt_fmt_u32", ffi::sub_rt_fmt_u32 as *const u8),
        ("sub_rt_fmt_i64", ffi::sub_rt_fmt_i64 as *const u8),
        ("sub_rt_fmt_u64", ffi::sub_rt_fmt_u64 as *const u8),
        ("sub_rt_fmt_f32", ffi::sub_rt_fmt_f32 as *const u8),
        ("sub_rt_fmt_f64", ffi::sub_rt_fmt_f64 as *const u8),
        ("sub_rt_fmt_bool", ffi::sub_rt_fmt_bool as *const u8),
        ("sub_rt_array_new", ffi::sub_rt_array_new as *const u8),
        ("sub_rt_array_len", ffi::sub_rt_array_len as *const u8),
        ("sub_rt_array_push", ffi::sub_rt_array_push as *const u8),
        ("sub_rt_array_pop", ffi::sub_rt_array_pop as *const u8),
        ("sub_rt_array_ptr", ffi::sub_rt_array_ptr as *const u8),
        ("sub_rt_str_data", ffi::sub_rt_str_data as *const u8),
        ("sub_rt_array_data", ffi::sub_rt_array_data as *const u8),
        ("sub_rt_cb_bind", ffi::sub_rt_cb_bind as *const u8),
        ("sub_rt_cb_trampoline", ffi::sub_rt_cb_trampoline as *const u8),
        // Math intrinsics (stdlib.md §1): the ship tier resolves the
        // same opaque symbols from the runtime static library.
        ("sub_rt_math_abs", ffi::sub_rt_math_abs as *const u8),
        ("sub_rt_math_acos", ffi::sub_rt_math_acos as *const u8),
        ("sub_rt_math_acosh", ffi::sub_rt_math_acosh as *const u8),
        ("sub_rt_math_asin", ffi::sub_rt_math_asin as *const u8),
        ("sub_rt_math_asinh", ffi::sub_rt_math_asinh as *const u8),
        ("sub_rt_math_atan", ffi::sub_rt_math_atan as *const u8),
        ("sub_rt_math_atanh", ffi::sub_rt_math_atanh as *const u8),
        ("sub_rt_math_cbrt", ffi::sub_rt_math_cbrt as *const u8),
        ("sub_rt_math_ceil", ffi::sub_rt_math_ceil as *const u8),
        ("sub_rt_math_cos", ffi::sub_rt_math_cos as *const u8),
        ("sub_rt_math_cosh", ffi::sub_rt_math_cosh as *const u8),
        ("sub_rt_math_exp", ffi::sub_rt_math_exp as *const u8),
        ("sub_rt_math_expm1", ffi::sub_rt_math_expm1 as *const u8),
        ("sub_rt_math_floor", ffi::sub_rt_math_floor as *const u8),
        ("sub_rt_math_log", ffi::sub_rt_math_log as *const u8),
        ("sub_rt_math_log1p", ffi::sub_rt_math_log1p as *const u8),
        ("sub_rt_math_log10", ffi::sub_rt_math_log10 as *const u8),
        ("sub_rt_math_log2", ffi::sub_rt_math_log2 as *const u8),
        ("sub_rt_math_round", ffi::sub_rt_math_round as *const u8),
        ("sub_rt_math_sign", ffi::sub_rt_math_sign as *const u8),
        ("sub_rt_math_sin", ffi::sub_rt_math_sin as *const u8),
        ("sub_rt_math_sinh", ffi::sub_rt_math_sinh as *const u8),
        ("sub_rt_math_sqrt", ffi::sub_rt_math_sqrt as *const u8),
        ("sub_rt_math_tan", ffi::sub_rt_math_tan as *const u8),
        ("sub_rt_math_tanh", ffi::sub_rt_math_tanh as *const u8),
        ("sub_rt_math_trunc", ffi::sub_rt_math_trunc as *const u8),
        ("sub_rt_math_atan2", ffi::sub_rt_math_atan2 as *const u8),
        ("sub_rt_math_hypot", ffi::sub_rt_math_hypot as *const u8),
        ("sub_rt_math_pow", ffi::sub_rt_math_pow as *const u8),
        ("sub_rt_math_max", ffi::sub_rt_math_max as *const u8),
        ("sub_rt_math_min", ffi::sub_rt_math_min as *const u8),
        ("sub_rt_math_random", ffi::sub_rt_math_random as *const u8),
        // Date intrinsics (stdlib.md §3): same opaque-symbol rule; the
        // ship tier resolves these from the runtime static library.
        // String method intrinsics (stdlib.md §8): one opaque symbol
        // per accepted method, StrFn::ALL order.
        ("sub_rt_str_index_of", ffi::sub_rt_str_index_of as *const u8),
        (
            "sub_rt_str_last_index_of",
            ffi::sub_rt_str_last_index_of as *const u8,
        ),
        ("sub_rt_str_includes", ffi::sub_rt_str_includes as *const u8),
        (
            "sub_rt_str_starts_with",
            ffi::sub_rt_str_starts_with as *const u8,
        ),
        ("sub_rt_str_ends_with", ffi::sub_rt_str_ends_with as *const u8),
        (
            "sub_rt_str_char_code_at",
            ffi::sub_rt_str_char_code_at as *const u8,
        ),
        ("sub_rt_str_split", ffi::sub_rt_str_split as *const u8),
        ("sub_rt_str_trim", ffi::sub_rt_str_trim as *const u8),
        ("sub_rt_str_trim_start", ffi::sub_rt_str_trim_start as *const u8),
        ("sub_rt_str_trim_end", ffi::sub_rt_str_trim_end as *const u8),
        ("sub_rt_str_repeat", ffi::sub_rt_str_repeat as *const u8),
        ("sub_rt_str_pad_start", ffi::sub_rt_str_pad_start as *const u8),
        ("sub_rt_str_pad_end", ffi::sub_rt_str_pad_end as *const u8),
        ("sub_rt_str_to_upper", ffi::sub_rt_str_to_upper as *const u8),
        ("sub_rt_str_to_lower", ffi::sub_rt_str_to_lower as *const u8),
        ("sub_rt_str_replace", ffi::sub_rt_str_replace as *const u8),
        (
            "sub_rt_str_replace_all",
            ffi::sub_rt_str_replace_all as *const u8,
        ),
        ("sub_rt_date_utc", ffi::sub_rt_date_utc as *const u8),
        ("sub_rt_date_new", ffi::sub_rt_date_new as *const u8),
        ("sub_rt_date_now", ffi::sub_rt_date_now as *const u8),
        ("sub_rt_date_get", ffi::sub_rt_date_get as *const u8),
        ("sub_rt_date_to_iso", ffi::sub_rt_date_to_iso as *const u8),
    ];
    for (name, addr) in syms {
        builder.symbol(*name, *addr);
    }
    register_interop(builder);
}

// The synthetic-header implementation, linked into this process by
// `build.rs` (from `corpus/interop/interop.c`). Only the addresses are
// taken — generated JIT code calls these under the C-ABI signatures the
// foreign-call lowering builds — so the declared Rust signatures are
// deliberately argument-less; a mismatch is irrelevant to address-taking
// and these are never called from Rust.
extern "C" {
    fn subDeviceCreate();
    fn subDeviceRetain();
    fn subDeviceRelease();
    fn subDeviceSubmit();
    fn subDeviceSetLogger();
    fn subDeviceSetLabel();
    fn subSliceChecksumF32();
    fn subSliceChecksumI32();
    fn subSliceChecksumF64();
    fn subSliceChecksumI64();
    fn subDrawListTotal();
    fn subAccessMatches();
    fn subBulkConsume();
    fn subBulkConsumeF32();
    fn subDeviceOnComplete();
    fn subDevicePump();
    fn subCommandBufferTotal();
    fn subStageMatches();
    fn subFutureMake();
    fn subStatsMake();
    fn subDeviceQuery();
    fn subDeviceKickAsync();
    fn subDeviceWait();
}

/// Registers the foreign C-header symbols (`corpus/interop/interop.c`,
/// linked by `build.rs`) so a `Callee::Foreign` call resolves at JIT
/// time, the same way the ship-C tier resolves them from the linked
/// object (compiler.md §12.4).
pub(crate) fn register_interop(builder: &mut JITBuilder) {
    let syms: &[(&str, *const u8)] = &[
        ("subDeviceCreate", subDeviceCreate as *const u8),
        ("subDeviceRetain", subDeviceRetain as *const u8),
        ("subDeviceRelease", subDeviceRelease as *const u8),
        ("subDeviceSubmit", subDeviceSubmit as *const u8),
        ("subDeviceSetLogger", subDeviceSetLogger as *const u8),
        ("subDeviceSetLabel", subDeviceSetLabel as *const u8),
        ("subSliceChecksumF32", subSliceChecksumF32 as *const u8),
        ("subSliceChecksumI32", subSliceChecksumI32 as *const u8),
        ("subSliceChecksumF64", subSliceChecksumF64 as *const u8),
        ("subSliceChecksumI64", subSliceChecksumI64 as *const u8),
        ("subDrawListTotal", subDrawListTotal as *const u8),
        ("subAccessMatches", subAccessMatches as *const u8),
        ("subBulkConsume", subBulkConsume as *const u8),
        ("subBulkConsumeF32", subBulkConsumeF32 as *const u8),
        ("subDeviceOnComplete", subDeviceOnComplete as *const u8),
        ("subDevicePump", subDevicePump as *const u8),
        ("subCommandBufferTotal", subCommandBufferTotal as *const u8),
        ("subStageMatches", subStageMatches as *const u8),
        ("subFutureMake", subFutureMake as *const u8),
        ("subStatsMake", subStatsMake as *const u8),
        ("subDeviceQuery", subDeviceQuery as *const u8),
        ("subDeviceKickAsync", subDeviceKickAsync as *const u8),
        ("subDeviceWait", subDeviceWait as *const u8),
    ];
    for (name, addr) in syms {
        builder.symbol(*name, *addr);
    }
}

/// Checks `files`, lowers the typed HIR through the shared CLIF
/// lowering, and finalizes the code in a live JIT module.
fn compile_jit(files: &[SourceFile]) -> Result<(JITModule, Lowered), RunError> {
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
    let mut module = JITModule::new(builder);

    let lowered = lower_module_with(&mut module, &hir, LowerOptions::default())
        .map_err(RunError::Internal)?;
    module
        .finalize_definitions()
        .map_err(|e| RunError::Internal(internal(format!("finalize: {e}"))))?;
    Ok((module, lowered))
}

/// Runs the module initializer and then the exported `main` on a fresh
/// Context, returning the stdout bytes of the run and how long the
/// `main` call itself took.
///
/// The initializer is deliberately outside the measured span: it is the
/// module's global setup, not the workload (`specs/blocks/compiler.md`
/// §9). Running it before every call also restores the module globals,
/// so repeated calls of the same `main` are the same computation.
fn run_entry(module: &JITModule, lowered: &Lowered) -> Result<(Vec<u8>, Duration), RunError> {
    let init_ptr = module.get_finalized_function(lowered.init);
    let main_ptr = module.get_finalized_function(lowered.main);

    let mut ctx = Context::new();
    let mut elapsed = Duration::ZERO;
    {
        type Entry = unsafe extern "C" fn(*mut Context);
        // SAFETY: `init_ptr`/`main_ptr` are finalized JIT code for
        // functions the lowering built with exactly this signature
        // (`(ctx) -> void`, host C calling convention); the module
        // outlives both calls; `ctx` is a live exclusive Context.
        // Generated code never unwinds (traps return through the
        // flag-check paths), so no panic crosses this boundary.
        unsafe {
            let init: Entry = std::mem::transmute(init_ptr);
            init(&mut *ctx);
            if !ctx.trapped() {
                let main: Entry = std::mem::transmute(main_ptr);
                let start = Instant::now();
                main(&mut *ctx);
                elapsed = start.elapsed();
            }
        }
    }

    match ctx.trap_record() {
        Some(r) => {
            let pos = lowered
                .positions
                .get(r.pos_id as usize)
                .cloned()
                .unwrap_or_else(|| Pos::new(String::new(), 0, 0));
            Err(RunError::Trap(TrapReport {
                rule: r.kind,
                message: r.message.clone(),
                pos,
            }))
        }
        None => Ok((ctx.take_stdout(), elapsed)),
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
/// position), [`RunError::Internal`] on backend failures.
pub fn run_jit(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    let (module, lowered) = compile_jit(files)?;
    let outcome = run_entry(&module, &lowered).map(|(out, _)| out);
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
    if timed == 0 {
        return Err(RunError::Internal(internal(
            "a benchmark subject needs at least one timed run",
        )));
    }
    let started = Instant::now();
    let (module, lowered) = compile_jit(files)?;
    let compile = started.elapsed();

    let mut samples = Vec::with_capacity(timed);
    let mut stdout: Option<Vec<u8>> = None;
    let mut failure: Option<RunError> = None;
    for run in 0..warmup + timed {
        match run_entry(&module, &lowered) {
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
                if run >= warmup {
                    samples.push(elapsed);
                }
            }
            Err(e) => {
                failure = Some(e);
                break;
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
        compile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(src: &str) -> Vec<SourceFile> {
        vec![SourceFile::new("test.ts", src)]
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
    fn jit_bench_needs_a_timed_run() {
        let err = jit_bench(&sources("export function main(): void {}\n"), 1, 0);
        assert!(matches!(err, Err(RunError::Internal(_))));
    }

    #[test]
    fn date_now_reads_the_pinned_context_clock_in_the_dev_tier() {
        // stdlib.md §3: `Date.now()` is Context-owned and pinnable. The
        // public `run_jit` builds its own Context, so this drives the
        // compiled entries directly on a Context whose clock is pinned.
        // Both tiers call the identical `sub_rt_date_now` symbol; the
        // ship tier's link resolves it from the same runtime. The
        // ship-tier half of the both-tier check is
        // `tests/cemit.rs::date_now_reads_the_pinned_context_clock_in_the_ship_tier`
        // — the same program, pinned ms, and expected bytes.
        let (module, lowered) = compile_jit(&sources(
            "export function main(): void {\n  const t: i64 = Date.now();\n  print(`${t}`);\n  print(new Date(Date.now()).toISOString());\n}\n",
        ))
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
