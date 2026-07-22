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
    fn jit_bench_surfaces_a_trap() {
        let err = jit_bench(
            &sources("export function main(): void {\n  const xs: i32[] = [];\n  xs.pop();\n}\n"),
            0,
            1,
        );
        assert!(matches!(err, Err(RunError::Trap(_))));
    }
}
