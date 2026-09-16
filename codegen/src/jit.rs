//! The dev-tier JIT driver: instantiates the Cranelift lowering
//! with `cranelift-jit`, resolves the runtime's `extern "C"` symbols,
//! runs the exported `main(): void`, and returns captured stdout bytes or a
//! trap report. Run helpers retain completed lines in a helper-owned file so
//! an abnormal child termination can return those bytes to the caller.

use subscript_compiler::{Diagnostic, Pos};
use subscript_runtime::TrapKind;

mod bench;
mod compile;
mod entry;
mod output;
#[cfg(test)]
mod probe;
mod run;
mod symbols;

pub use self::bench::{
    jit_bench, jit_bench_configured, jit_bench_with_warmup_floor, jit_compile_time, BenchSamples,
};
#[cfg(test)]
pub(crate) use self::probe::{
    allocation_attribution_after_run, live_allocations_after_main_calls,
    memory_accounting_after_run,
};
pub use self::run::{
    run_jit, run_jit_configured, run_jit_interrupted, run_jit_with_alloc_failure,
    run_jit_with_freed_handle_diagnostics_and_native_libraries, run_jit_with_memory_accounting,
    run_jit_with_memory_accounting_and_native_libraries, run_jit_with_native_libraries,
};
pub(crate) use self::symbols::register_runtime;

/// Optional environment-variable override naming an existing parent-owned
/// output file. JIT run helpers otherwise create and own a temporary file;
/// either way, every completed line is appended and flushed before execution
/// continues.
pub const JIT_OUTPUT_FILE_ENV: &str = "SUBSCRIPT_CODEGEN_JIT_OUTPUT_FILE";

/// A run ended outside the runtime trap protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AbnormalTermination {
    /// Platform description of the exit status or signal.
    pub status: String,
    /// Exact stdout bytes produced before termination.
    pub stdout: Vec<u8>,
    /// Exact stderr bytes captured before termination.
    pub stderr: Vec<u8>,
}

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
    /// The program was rejected by the checker.
    Rejected(Vec<Diagnostic>),
    /// The program ran and trapped.
    Trap(TrapReport),
    /// A called foreign C symbol was absent from every caller-supplied
    /// native library.
    UnresolvedForeignSymbol(String),
    /// Generated or foreign code ended by a signal or another abnormal
    /// process termination outside the runtime trap protocol.
    AbnormalTermination(AbnormalTermination),
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
            RunError::AbnormalTermination(termination) => {
                write!(f, "program terminated abnormally ({})", termination.status)?;
                if !termination.stderr.is_empty() {
                    write!(f, ": {}", String::from_utf8_lossy(&termination.stderr))?;
                }
                Ok(())
            }
            RunError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for RunError {}
#[cfg(test)]
mod tests {
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;

    use subscript_compiler::{Profile, SourceFile};
    use subscript_runtime::{ffi, Context, Interrupt, TrapKind};

    use super::compile::compile_jit;
    use super::*;
    use crate::RunConfig;

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
        let (module, lowered, _) =
            compile_jit(&program, &[], Profile::Default).expect("compile allocation probe");
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
        assert!(
            !ctx.trapped(),
            "probe setup trapped: {:?}",
            ctx.trap_record()
        );
        // SAFETY: shared access after the setup entry returned.
        let before = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        // SAFETY: finalized allocation-probe entry.
        unsafe { call_entry(iterate, &mut ctx) };
        assert!(
            !ctx.trapped(),
            "fused loop trapped: {:?}",
            ctx.trap_record()
        );
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
    fn set_source_construction_allocates_only_the_set_storage() {
        // compiler.md §103.1 rule 4 and the §103.7 gate: the fused
        // traversal costs no allocation beyond the Set's own storage.
        // The control is the loop the construction stands for, written
        // out; the two live-allocation deltas must agree.
        let program = sources(
            "const values: i32[] = [1, 2, 3, 2];\n\
             const text: string = \"Aé漢é\";\n\
             let fused: i32 = 0;\n\
             let manual: i32 = 0;\n\
             export function build_fused(): void {\n\
               const numbers: Set<i32> = new Set<i32>(values);\n\
               const points: Set<string> = new Set<string>(text);\n\
               fused += numbers.size + points.size;\n\
             }\n\
             export function build_manual(): void {\n\
               const numbers: Set<i32> = new Set<i32>();\n\
               for (const value of values) { numbers.add(value); }\n\
               const points: Set<string> = new Set<string>();\n\
               for (const codePoint of text) { points.add(codePoint); }\n\
               manual += numbers.size + points.size;\n\
             }\n\
             export function build_with_one_extra_array(): void {\n\
               const numbers: Set<i32> = new Set<i32>();\n\
               const staging: i32[] = [...values];\n\
               for (const value of staging) { numbers.add(value); }\n\
               const points: Set<string> = new Set<string>();\n\
               for (const codePoint of text) { points.add(codePoint); }\n\
               manual += numbers.size + points.size + staging.length;\n\
             }\n\
             export function main(): void {}\n",
        );
        let (module, lowered, _) =
            compile_jit(&program, &[], Profile::Default).expect("compile allocation probe");
        let init = module.get_finalized_function(lowered.init);
        let entry = |name: &str| {
            lowered
                .entries
                .iter()
                .find(|entry| entry.name == name)
                .map(|entry| module.get_finalized_function(entry.id))
                .expect("probe entry")
        };
        let fused = entry("build_fused");
        let manual = entry("build_manual");
        let staged = entry("build_with_one_extra_array");
        let mut ctx = Context::new();
        let p: *const Context = &*ctx;
        // SAFETY: the finalized init entry and the module stay alive.
        unsafe { call_entry(init, &mut ctx) };
        assert!(
            !ctx.trapped(),
            "probe setup trapped: {:?}",
            ctx.trap_record()
        );
        // SAFETY: shared access after the setup entry returned.
        let before_fused = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        // SAFETY: finalized allocation-probe entry.
        unsafe { call_entry(fused, &mut ctx) };
        // SAFETY: shared access after the probe entry returned.
        let after_fused = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        // SAFETY: finalized allocation-probe entry.
        unsafe { call_entry(manual, &mut ctx) };
        // SAFETY: shared access after the probe entry returned.
        let after_manual = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        // SAFETY: finalized allocation-probe entry.
        unsafe { call_entry(staged, &mut ctx) };
        // SAFETY: shared access after the probe entry returned.
        let after_staged = unsafe { ffi::subscript_rt_ctx_live_allocations(p) };
        assert!(
            !ctx.trapped(),
            "allocation probe trapped: {:?}",
            ctx.trap_record()
        );
        assert_eq!(
            after_fused - before_fused,
            after_manual - after_fused,
            "`new Set(source)` allocated more than the loop it stands for"
        );
        // The control: one staging array is one more live allocation, so
        // the comparison above reports a materialized source.
        assert!(
            after_staged - after_manual > after_fused - before_fused,
            "the probe does not see an added allocation"
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
        let (module, lowered, _) =
            compile_jit(&program, &[], Profile::Default).expect("compile observer program");
        let init = module.get_finalized_function(lowered.init);
        let main = module.get_finalized_function(lowered.main_id().expect("main entry"));
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
        assert_eq!(observed.pos_id, unsafe {
            ffi::subscript_rt_ctx_trap_pos_id(p)
        });
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
        let (module, lowered, _) =
            compile_jit(&program, &[], Profile::Default).expect("compile a01");
        let init = module.get_finalized_function(lowered.init);
        let main = module.get_finalized_function(lowered.main_id().expect("main entry"));

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
            let (stdout, accounting) =
                run_jit_with_memory_accounting(&[SourceFile::new("accounting.ts", source)], true)
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
        let (module, lowered, _) = compile_jit(
            &sources(
                "export function main(): void {\n  const t: i64 = Date.now();\n  print(`${t}`);\n  print(new Date(Date.now()).toISOString());\n}\n",
            ),
            &[],
            Profile::Default,
        )
        .expect("compile");
        let init_ptr = module.get_finalized_function(lowered.init);
        let main_ptr = module.get_finalized_function(lowered.main_id().expect("main entry"));
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

    /// §109.5: the bench runner applies the profile defaults, so a
    /// sandbox-profile run stops at the stack budget. The same program with
    /// a base case is the firing control: it runs clean under the profile.
    #[test]
    fn jit_bench_configured_applies_the_profile_defaults() {
        const ENDLESS: &str = "function descend(depth: i32): i32 {\n                                 return descend(depth + 1) + 1;\n}\n                               export function main(): void {\n                                 print(`${descend(0)}`);\n}\n";
        const BOUNDED: &str = "function descend(depth: i32): i32 {\n                                 if (depth > 8) {\n    return depth;\n  }\n                                 return descend(depth + 1) + 1;\n}\n                               export function main(): void {\n                                 print(`${descend(0)}`);\n}\n";
        let config = RunConfig::with_profile(Profile::Sandbox);
        let trapped = jit_bench_configured(&sources(ENDLESS), config, 0, 1, Duration::ZERO);
        match trapped {
            Err(RunError::Trap(report)) => assert_eq!(report.rule, TrapKind::StackBudget),
            other => panic!("expected the stack-budget trap, got {other:?}"),
        }
        let clean = jit_bench_configured(&sources(BOUNDED), config, 0, 2, Duration::ZERO)
            .expect("the bounded program runs under the profile");
        assert_eq!(clean.stdout, b"18\n");
        assert_eq!(clean.samples.len(), 2);
    }

    /// A benchmark reports timed samples only, so the options with no
    /// channel here are refused rather than ignored.
    #[test]
    fn jit_bench_configured_refuses_an_option_it_cannot_report() {
        let files = sources("export function main(): void {\n  print(\"tick\");\n}\n");
        let sink: OnceLock<Arc<Interrupt>> = OnceLock::new();
        for config in [
            RunConfig {
                memory_accounting: true,
                ..RunConfig::default()
            },
            RunConfig {
                interrupt_after_millis: Some(1),
                ..RunConfig::default()
            },
            RunConfig {
                interrupt_handle: Some(&sink),
                ..RunConfig::default()
            },
            RunConfig {
                pre_entry_hook: Some("host_pre_entry"),
                ..RunConfig::default()
            },
        ] {
            let refused = jit_bench_configured(&files, config, 0, 1, Duration::ZERO);
            assert!(
                matches!(refused, Err(RunError::Internal(_))),
                "the runner accepted an option it cannot report"
            );
        }
        // The firing control: the same call with the default record runs.
        let ran = jit_bench_configured(&files, RunConfig::default(), 0, 1, Duration::ZERO)
            .expect("the default record runs");
        assert_eq!(ran.stdout, b"tick\n");
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
