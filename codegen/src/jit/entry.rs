//! Execution of one dev-tier entry: the options a run takes, the Context
//! it runs on, and the retained-output child on Unix.

use std::ffi::c_void;
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
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
#[cfg(unix)]
use crate::lower::internal;
use crate::lower::Lowered;

pub(super) struct CompletedRun {
    pub(super) ctx: Box<Context>,
    pub(super) stdout: Vec<u8>,
    pub(super) elapsed: Duration,
}

/// What one dev-tier entry run needs beyond the finalized code.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EntryOptions {
    pub(super) file_provider: Option<subscript_runtime::ffi::FileProvider>,
    /// Object-level Context allocation number to reject.
    pub(super) fail_alloc_after: Option<u64>,
    /// Enables retained freed-handle diagnostics.
    pub(super) freed_handle_diagnostics: bool,
}

/// Runs the module initializer and then the exported `main` on a fresh
/// Context, returning the stdout bytes of the run and how long the
/// `main` call itself took.
///
/// The initializer is deliberately outside the measured span: it is the
/// module's global setup, not the workload (`specs/blocks/compiler.md`
/// §9). Running it before every call also restores the module globals,
/// so repeated calls of the same `main` are the same computation.
pub(super) fn execute_entry(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
    write_through: Option<File>,
) -> Result<CompletedRun, RunError> {
    let EntryOptions {
        file_provider,
        fail_alloc_after,
        freed_handle_diagnostics,
    } = options;
    let init_ptr = module.get_finalized_function(lowered.init);
    let main = match lowered.main_id() {
        Ok(main) => main,
        Err(message) => return Err(RunError::Internal(message)),
    };
    let main_ptr = module.get_finalized_function(main);

    let needs_panic_stdout_fallback = write_through.is_none();
    let mut ctx = Context::new();
    unsafe {
        ctx.set_file_provider(file_provider);
    }
    let mut stdout = Box::new(CapturedStdout {
        bytes: Vec::new(),
        write_through,
    });
    ctx.set_print_observer(
        Some(capture_stdout_line),
        (&mut *stdout as *mut CapturedStdout).cast::<c_void>(),
    );
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
                let runner = module.get_finalized_function(lowered.async_runner);
                call_script_entry(runner, &mut ctx);
            }
            while !ctx.trapped() && ctx.async_pending() != 0 {
                // SAFETY: every pending item was registered by a generated
                // async export wrapper in this finalized module.
                ctx.async_step();
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
    ctx.set_print_observer(None, std::ptr::null_mut());
    drop(aborting_stdout);
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
        }),
    }
}

/// Runs the entry in a forked child and returns its retained output.
///
/// compiler.md §190.1 rule 1: the `fork` happens only when the process
/// has one thread. The caller selects this path from the same check;
/// the check here is the last one before the `fork`. `settle` marks a run
/// with a native library or a file provider, whose count above 1 is read
/// again for up to 2 ms (rule 3).
#[cfg(unix)]
pub(super) fn execute_entry_retained(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
    settle: bool,
) -> Result<Vec<u8>, RunError> {
    let mut output = RetainedOutput::new()?;
    let writer = output.writer()?;
    let mut protocol = TemporaryFile::new("protocol")?;
    let mut stderr = TemporaryFile::new("stderr")?;

    let threads = isolation::thread_count_for_run(settle);
    if threads != Some(1) {
        return Err(RunError::Internal(internal(format!(
            "{} (the process has {} threads)",
            isolation::RULE_1_MESSAGE,
            isolation::describe_count(threads)
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
        let stderr_fd = stderr
            .file
            .as_ref()
            .expect("live JIT child stderr")
            .as_raw_fd();
        // SAFETY: both descriptors are live in the forked child. Redirecting
        // stderr keeps panic and signal diagnostics with the returned error.
        if unsafe { libc::dup2(stderr_fd, libc::STDERR_FILENO) } < 0 {
            // SAFETY: this is the forked child and redirection failed before
            // generated code ran.
            unsafe { libc::_exit(122) };
        }
        let outcome = execute_entry(module, lowered, options, Some(writer));
        let written = write_outcome(
            protocol.file.as_mut().expect("live JIT child protocol"),
            &outcome,
        )
        .is_ok();
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
    let mut status = 0;
    loop {
        // SAFETY: `child` is the positive PID of a child of this process;
        // `status` is live writable storage and this parent waits for that
        // child only.
        let waited = unsafe { libc::waitpid(child, &mut status, 0) };
        if waited == child {
            break;
        }
        if waited < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return Err(RunError::Internal(internal(format!(
            "wait for JIT child: {}",
            std::io::Error::last_os_error()
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
    let output = RetainedOutput::new()?;
    let writer = output.writer()?;
    execute_entry(module, lowered, options, Some(writer)).map(|run| run.stdout)
}

pub(super) fn run_entry(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions,
) -> Result<(Vec<u8>, Duration), RunError> {
    execute_entry(module, lowered, options, None).map(|run| (run.stdout, run.elapsed))
}

pub(super) fn memory_accounting(ctx: &Context) -> JitMemoryAccounting {
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
