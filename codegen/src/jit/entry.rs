//! Execution of one dev-tier entry: the options a run takes, the Context
//! it runs on, and the retained-output child on Unix.

use std::ffi::c_void;
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cranelift_jit::JITModule;
use subscript_runtime::{ffi, Context, FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES};

use super::compile::call_script_entry;
#[cfg(unix)]
use super::isolation;
#[cfg(unix)]
use super::output::TemporaryFile;
use super::output::{capture_stdout_line, AbortingStdoutGuard, CapturedStdout, RetainedOutput};
#[cfg(unix)]
use super::protocol::{parse_outcome, write_outcome};
#[cfg(unix)]
use super::AbnormalTermination;
use super::{JitMemoryAccounting, RunError, TrapReport};
use crate::lower::internal;
use crate::lower::Lowered;

pub(super) struct CompletedRun {
    pub(super) ctx: Box<Context>,
    pub(super) stdout: Vec<u8>,
    pub(super) elapsed: Duration,
    /// Thread CPU time over the span of `elapsed`, on a host with that clock.
    pub(super) cpu: Option<Duration>,
}

/// The CPU time of the calling thread (`compiler.md` §213 rule 1a). A host
/// with no such clock gives `None`.
fn thread_cpu_time() -> Option<Duration> {
    #[cfg(unix)]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `time` is live writable storage for this one call.
        let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
        if status != 0 {
            return None;
        }
        Some(Duration::new(
            u64::try_from(time.tv_sec).ok()?,
            u32::try_from(time.tv_nsec).ok()?,
        ))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// What one dev-tier entry run needs beyond the finalized code.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EntryOptions {
    pub(super) file_provider: Option<subscript_runtime::ffi::FileProvider>,
    /// Object-level Context allocation number to reject.
    pub(super) fail_alloc_after: Option<u64>,
    /// Enables retained freed-handle diagnostics.
    pub(super) freed_handle_diagnostics: bool,
    /// Collection mode and minimum headroom (`compiler.md` §342 rule 17).
    pub(super) collection: Option<(subscript_runtime::context::CollectionMode, usize)>,
}

/// Runs the module initializer and then the exported `main` on a fresh
/// Context, returning the stdout bytes of the run and how long the
/// `main` call itself took, in wall time and in thread CPU time.
///
/// The initializer is deliberately outside the measured span: it is the
/// module's global setup, not the workload (`specs/blocks/compiler.md`
/// §9). Running it before every call also restores the module globals,
/// so repeated calls of the same `main` are the same computation.
pub(super) fn execute_entry(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
    write_through: Option<(File, PathBuf)>,
) -> Result<CompletedRun, RunError> {
    let EntryOptions {
        file_provider,
        fail_alloc_after,
        freed_handle_diagnostics,
        collection,
    } = options;
    let init_ptr = module.get_finalized_function(lowered.init);
    let main = match lowered.main_id() {
        Ok(main) => main,
        Err(message) => return Err(RunError::Internal(message)),
    };
    let main_ptr = module.get_finalized_function(main);

    let needs_panic_stdout_fallback = write_through.is_none();
    let mut ctx = Context::new();
    // SAFETY: with_file_provider requires host state and callbacks to outlive this run (section 263).
    unsafe {
        ctx.set_file_provider(file_provider);
    }
    let mut stdout = Box::new(CapturedStdout {
        bytes: Vec::new(),
        write_through,
        write_error: None,
    });
    unsafe {
        // SAFETY: capture_stdout_line is static; the stdout Box stays live until observer removal (section 263).
        ctx.set_print_observer(
            Some(capture_stdout_line),
            (&mut *stdout as *mut CapturedStdout).cast::<c_void>(),
        )
    };
    // The hook flushes the buffer the observer fills.
    let aborting_stdout =
        needs_panic_stdout_fallback.then(|| AbortingStdoutGuard::install(&stdout.bytes));
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
    if let Some((mode, min_headroom_bytes)) = collection {
        // A fresh Context has script depth zero, so the change applies.
        if !ctx.set_collection(mode, min_headroom_bytes) {
            return Err(RunError::Internal(internal(
                "the Context refused the collection mode",
            )));
        }
    }
    let mut elapsed = Duration::ZERO;
    let mut cpu = None;
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
                let cpu_start = thread_cpu_time();
                let start = Instant::now();
                main(&mut *ctx);
                elapsed = start.elapsed();
                cpu = thread_cpu_time()
                    .zip(cpu_start)
                    .and_then(|(end, start)| end.checked_sub(start));
                ctx.exit_script();
            }
            if !ctx.trapped() {
                let runner = module.get_finalized_function(lowered.async_runner);
                call_script_entry(runner, &mut ctx);
            }
            // §349 rule 11: this driver owns its loop, so it waits on the
            // receive signal when no task is pending, and steps again if
            // the wait returned after the signal.
            loop {
                while !ctx.trapped() && ctx.async_pending() != 0 {
                    // SAFETY: every pending item was registered by a generated
                    // async export wrapper in this finalized module.
                    ctx.async_step();
                }
                if !ctx.async_wait() {
                    break;
                }
            }
        }
    }

    let trap = ctx.trap_record().map(|r| {
        // §112 rule 1: the recorded id resolves through the lowered
        // table, and id 0 is its reserved entry, so both tiers report
        // the same position.
        let pos = lowered.positions.report_position(r.pos_id);
        (r.kind, r.message.clone(), pos)
    });
    unsafe {
        // SAFETY: capture_stdout_line is static; the stdout Box stays live until observer removal (section 263).
        ctx.set_print_observer(None, std::ptr::null_mut())
    };
    drop(aborting_stdout);
    // compiler.md §268 rules 2 and 3: report the file error and any trap.
    if let (Some(error), Some((_, path))) = (stdout.write_error, &stdout.write_through) {
        let mut message = format!("write retained JIT output file {}: {error}", path.display());
        if let Some((rule, trap_message, pos)) = trap {
            message.push_str(&format!("; {pos}: trap [{rule}]: {trap_message}"));
        }
        return Err(RunError::Internal(internal(message)));
    }
    let stdout = stdout.bytes;
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
            cpu,
        }),
    }
}

/// Runs the entry in a forked child and returns its retained output.
///
/// compiler.md §190.1 rule 1: the `fork` happens only when the process
/// has one thread. The caller selects this path from the same check;
/// the check here is the last one before the `fork`. `settle` marks a run
/// with a native library or a file provider. On Apple targets its count
/// above 1 is read again for up to 2 ms; on Linux the count is read once
/// and leaves out an exiting thread (rule 3).
#[cfg(unix)]
pub(super) fn execute_entry_retained(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
    settle: bool,
) -> Result<Vec<u8>, RunError> {
    let mut output = RetainedOutput::open(
        std::env::var_os(super::JIT_OUTPUT_FILE_ENV)
            .as_deref()
            .map(std::path::Path::new),
    )?;
    let writer = output.writer()?;
    let mut protocol = TemporaryFile::new("protocol")?;
    let mut stderr = TemporaryFile::new("stderr")?;

    let stderr_fd = stderr.file_mut()?.as_raw_fd();
    let protocol_writer = protocol.file_mut()?;
    let threads = isolation::thread_count_for_run(settle);
    if threads != Some(1) {
        return Err(RunError::Internal(internal(isolation::thread_failure(
            isolation::RULE_1_MESSAGE,
            threads,
        ))));
    }
    // SAFETY: the process has one thread, so no other thread holds a lock
    // at the `fork` (§190.1 rule 1). The child inherits finalized JIT code,
    // the Context runtime, and caller-supplied native symbol addresses. It
    // reports through files and calls `_exit`, while the parent alone frees
    // the module after `waitpid`.
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(RunError::Internal(internal(format!(
            "fork JIT runner: {}",
            std::io::Error::last_os_error()
        ))));
    }
    if child == 0 {
        // SAFETY: both descriptors are live in the forked child. Redirecting
        // stderr keeps panic and signal diagnostics with the returned error.
        if unsafe { libc::dup2(stderr_fd, libc::STDERR_FILENO) } < 0 {
            // SAFETY: this is the forked child and redirection failed before
            // generated code ran.
            unsafe { libc::_exit(122) };
        }
        let outcome = execute_entry(module, lowered, options, Some(writer));
        let written = write_outcome(protocol_writer, &outcome).is_ok();
        // SAFETY: this is the forked child. `_exit` avoids running inherited
        // parent destructors or freeing the parent's JIT mappings.
        unsafe { libc::_exit(if written { 0 } else { 121 }) };
    }
    drop(writer);
    collect_child(child, &mut output, &mut stderr, &mut protocol)
}

/// Waits for a child that runs one entry, then reads its retained
/// stdout, its stderr, and its outcome. A forked child and a runner
/// report in the same form.
#[cfg(unix)]
pub(super) fn collect_child(
    child: libc::pid_t,
    output: &mut RetainedOutput,
    stderr: &mut TemporaryFile,
    protocol: &mut TemporaryFile,
) -> Result<Vec<u8>, RunError> {
    collect_child_with_wait(child, output, stderr, protocol, wait_child)
}

#[cfg(unix)]
fn wait_child(child: libc::pid_t, status: &mut libc::c_int) -> std::io::Result<libc::pid_t> {
    // SAFETY: the caller supplies a child PID and live status storage.
    let waited = unsafe { libc::waitpid(child, status, 0) };
    if waited < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(waited)
    }
}

#[cfg(unix)]
fn collect_child_with_wait(
    child: libc::pid_t,
    output: &mut RetainedOutput,
    stderr: &mut TemporaryFile,
    protocol: &mut TemporaryFile,
    mut wait: impl FnMut(libc::pid_t, &mut libc::c_int) -> std::io::Result<libc::pid_t>,
) -> Result<Vec<u8>, RunError> {
    let mut status = 0;
    loop {
        let error = match wait(child, &mut status) {
            Ok(waited) if waited == child => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => error,
            Ok(waited) => std::io::Error::other(format!("unexpected child PID {waited}")),
        };
        // SAFETY: `child` is the owned child PID. SIGKILL stops a live child.
        unsafe { libc::kill(child, libc::SIGKILL) };
        loop {
            match wait_child(child, &mut status) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                _ => break,
            }
        }
        return Err(RunError::Internal(internal(format!(
            "wait for JIT child: {error}"
        ))));
    }

    let stdout = output.bytes()?;
    let stderr = stderr.bytes()?;
    if libc::WIFSIGNALED(status) {
        return Err(RunError::AbnormalTermination(AbnormalTermination {
            status: format!("dev-JIT child signal {}", libc::WTERMSIG(status)),
            stdout,
            stderr,
        }));
    }
    if !libc::WIFEXITED(status) || libc::WEXITSTATUS(status) != 0 {
        let status = if libc::WIFEXITED(status) {
            format!("dev-JIT child exit {}", libc::WEXITSTATUS(status))
        } else {
            "dev-JIT child unknown status".to_string()
        };
        return Err(RunError::AbnormalTermination(AbnormalTermination {
            status,
            stdout,
            stderr,
        }));
    }
    let protocol = protocol.bytes()?;
    parse_outcome(&protocol, stdout)
}

#[cfg(not(unix))]
pub(super) fn execute_entry_retained(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
    _settle: bool,
) -> Result<Vec<u8>, RunError> {
    let output = RetainedOutput::open(
        std::env::var_os(super::JIT_OUTPUT_FILE_ENV)
            .as_deref()
            .map(std::path::Path::new),
    )?;
    let writer = output.writer()?;
    execute_entry(module, lowered, options, Some(writer)).map(|run| run.stdout)
}

pub(super) fn run_entry(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
) -> Result<(Vec<u8>, Duration, Option<Duration>), RunError> {
    execute_entry(module, lowered, options, None).map(|run| (run.stdout, run.elapsed, run.cpu))
}

pub(super) fn memory_accounting(ctx: &Context) -> JitMemoryAccounting {
    let p: *const Context = ctx;
    // SAFETY: shared host accessors over a live Context after every script
    // entry returned.
    unsafe {
        JitMemoryAccounting {
            live_bytes: ffi::subscript_rt_ctx_live_bytes(p),
            reserved_bytes: ffi::subscript_rt_ctx_reserved_bytes(p),
            automatic_collections: ctx.automatic_collection_count(),
            detection_collections: ctx.detection_collection_count(),
            outermost_exits: ctx.outermost_exit_count(),
            live_array_facts: ctx
                .live_array_facts()
                .into_iter()
                .map(|(len, elements)| {
                    (
                        len,
                        elements == subscript_runtime::context::ArrayElements::NoHandle,
                    )
                })
                .collect(),
        }
    }
}

#[cfg(all(test, unix))]
mod retained_output_tests {
    use super::super::compile::compile_jit;
    use super::*;
    use subscript_compiler::SourceFile;

    // compiler.md §268 rules 2 and 4: one small module and two entry calls.
    #[test]
    fn retained_output_write_error_fails_run() {
        let files = [SourceFile::new(
            "output.ts",
            "export function main(): void { print(`123`); print(`456`); }",
        )];
        let (module, lowered) = compile_jit(&files, &[]).unwrap();
        let mut output = RetainedOutput::open(None).unwrap();
        let readonly = File::open(output.path()).unwrap();
        let result = execute_entry(
            &module,
            &lowered,
            EntryOptions::default(),
            Some((readonly, output.path().to_path_buf())),
        );
        let expected_error =
            std::io::Write::write_all(&mut File::open(output.path()).unwrap(), b"x")
                .unwrap_err()
                .to_string();
        let error = result
            .err()
            .expect("retained write error must fail the run");
        assert!(matches!(error, RunError::Internal(_)));
        assert!(error.to_string().contains(&expected_error));
        assert!(error
            .to_string()
            .contains(&output.path().display().to_string()));
        let run = execute_entry(
            &module,
            &lowered,
            EntryOptions::default(),
            Some(output.writer().unwrap()),
        )
        .unwrap();
        assert_eq!(run.stdout, b"123\n456\n");
        assert_eq!(output.bytes().unwrap(), b"123\n456\n");
        drop(run);
        // SAFETY: no entry or Context refers to the finalized code.
        unsafe { module.free_memory() };
    }

    // compiler.md §268 rule 3: one small module also raises a script exception.
    #[test]
    fn retained_output_error_reports_trap() {
        let files = [SourceFile::new(
            "output.ts",
            "export function main(): void { print(`123`); throw new Error(`script failure`); }",
        )];
        let (module, lowered) = compile_jit(&files, &[]).unwrap();
        let output = RetainedOutput::open(None).unwrap();
        let result = execute_entry(
            &module,
            &lowered,
            EntryOptions::default(),
            Some((
                File::open(output.path()).unwrap(),
                output.path().to_path_buf(),
            )),
        );
        let error = result.err().expect("file error and trap");
        assert!(matches!(error, RunError::Internal(_)));
        let message = error.to_string();
        assert!(message.contains(&output.path().display().to_string()));
        assert!(message.contains("script failure"), "{message}");
        assert!(message.contains("trap ["), "{message}");
        // SAFETY: no entry or Context refers to the finalized code.
        unsafe { module.free_memory() };
    }
}

#[cfg(all(test, unix))]
mod wait_tests {
    use super::*;

    // compiler.md §340 rule 4: debug 9.1 ms; 0 C builds.
    #[test]
    fn a_wait_error_stops_and_reaps_a_live_child() {
        let measured = std::time::Instant::now();
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id() as libc::pid_t;
        let mut output = RetainedOutput::open(None).unwrap();
        let mut stderr = TemporaryFile::new("stderr").unwrap();
        let mut protocol = TemporaryFile::new("protocol").unwrap();
        let mut calls = 0;
        let outcome =
            collect_child_with_wait(pid, &mut output, &mut stderr, &mut protocol, |_, _| {
                calls += 1;
                Err(std::io::Error::from_raw_os_error(libc::EINVAL))
            });
        let mut status = 0;
        // SAFETY: pid identifies this test's child; status is live writable storage.
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        let error = std::io::Error::last_os_error();
        if waited == 0 {
            // SAFETY: the test owns this live child. Reap it even if the assertion fails.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
        }
        // compiler.md §340 rule 4: waitpid above supplies the independent reap observation.
        // Child also completes its ownership protocol after that observation and any cleanup.
        assert_eq!(child.wait().unwrap_err().raw_os_error(), Some(libc::ECHILD));
        assert_eq!(calls, 1);
        assert!(
            matches!(outcome, Err(RunError::Internal(ref message)) if message.contains("wait for JIT child")),
            "{outcome:?}"
        );
        assert_eq!(waited, -1, "the child was not reaped");
        assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
        eprintln!(
            "a_wait_error_stops_and_reaps_a_live_child cost: {:?}",
            measured.elapsed()
        );
    }
}
