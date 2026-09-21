//! The dev-tier measurement entry points of the performance gate
//! (`specs/blocks/compiler.md` §9).

use std::time::{Duration, Instant};

use subscript_compiler::SourceFile;

use super::compile::compile_jit;
use super::entry::{run_entry, EntryOptions};
use super::RunError;
use crate::lower::internal;
use crate::RunConfig;

/// Timed samples for one subject of the performance gate
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
    /// One observation of the time spent turning source into executable code
    /// before this execution batch. The §3 iteration gate uses repeated
    /// [`jit_compile_time`] observations instead.
    pub compile: Duration,
}

/// Measures one dev-tier iteration: check the changed source, lower it to
/// native code, and finalize the JIT module so it is ready to run.
///
/// The returned duration ends when finalization completes. Releasing the
/// temporary module and executing an entry are outside the measured span.
/// This is the iteration-time subject gated by `compiler.md` §3; callers that
/// need a statistically valid result repeat this whole one-shot operation
/// under §9's sampling methodology.
///
/// # Errors
///
/// Returns the same check, lowering, finalization, and symbol-resolution
/// errors as [`run_jit`].
pub fn jit_compile_time(files: &[SourceFile]) -> Result<Duration, RunError> {
    let started = Instant::now();
    let (module, _lowered) = compile_jit(files, &[])?;
    let elapsed = started.elapsed();
    // SAFETY: the finalized module was not executed and no pointer into its
    // code or data escaped this function.
    unsafe { module.free_memory() };
    Ok(elapsed)
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
    jit_bench_configured(files, RunConfig::default(), warmup, timed, warmup_floor)
}

/// Measures the dev-JIT tier like [`jit_bench_with_warmup_floor`], with one
/// complete option record.
///
/// A benchmark reports timed samples only, so `memory_accounting` and
/// the shipping-tier host hooks are unavailable here.
///
/// # Errors
///
/// Returns the same errors as [`jit_bench`], and [`RunError::Internal`] for
/// an option this runner has no channel for.
pub fn jit_bench_configured(
    files: &[SourceFile],
    config: RunConfig<'_>,
    warmup: usize,
    timed: usize,
    warmup_floor: Duration,
) -> Result<BenchSamples, RunError> {
    if timed == 0 {
        return Err(RunError::Internal(internal(
            "a benchmark subject needs at least one timed run",
        )));
    }
    if config.pre_entry_hook.is_some() || config.post_run_hook.is_some() {
        return Err(RunError::Internal(internal(
            "host hooks are not available in the development tier",
        )));
    }
    if config.memory_accounting {
        return Err(RunError::Internal(internal(
            "a benchmark reports timed samples only: memory accounting is \
             not available",
        )));
    }
    let started = Instant::now();
    let (module, lowered) = compile_jit(files, config.native_libraries)?;
    let compile = started.elapsed();
    let options = EntryOptions {
        fail_alloc_after: config.fail_alloc_after,
        freed_handle_diagnostics: config.freed_handle_diagnostics,
    };

    let mut samples = Vec::with_capacity(timed);
    let mut warmup_elapsed = Duration::ZERO;
    let mut warmup_iterations = 0;
    let mut stdout: Option<Vec<u8>> = None;
    let mut failure: Option<RunError> = None;
    while warmup_iterations < warmup || warmup_elapsed < warmup_floor {
        match run_entry(&module, &lowered, options) {
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
            match run_entry(&module, &lowered, options) {
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
