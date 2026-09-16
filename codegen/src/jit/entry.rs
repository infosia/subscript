//! Execution of one dev-tier entry: the options a run takes, the Context
//! it runs on, and the retained-output child on Unix.

use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use cranelift_jit::JITModule;
use subscript_compiler::{Pos, Profile};
use subscript_runtime::{
    ffi, Context, Interrupt, TrapKind, FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES,
};

use super::compile::call_script_entry;
use super::output::{
    capture_stdout_line, AbortingStdoutGuard, CapturedStdout, RetainedOutput, TemporaryFile,
};
use super::{AbnormalTermination, JitMemoryAccounting, RunError, TrapReport};
use crate::lower::{internal, Lowered};
use crate::HostLimits;

pub(super) struct CompletedRun {
    pub(super) ctx: Box<Context>,
    pub(super) stdout: Vec<u8>,
    pub(super) elapsed: Duration,
}

/// What one dev-tier entry run produced: its outcome, and the measured
/// interrupt latency when the run set the flag from a second thread.
pub(super) struct EntryOutcome {
    pub(super) run: Result<CompletedRun, RunError>,
    pub(super) interrupt_latency: Option<Duration>,
}

/// What one dev-tier entry run needs beyond the finalized code.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EntryOptions<'a> {
    /// Object-level Context allocation number to reject.
    pub(super) fail_alloc_after: Option<u64>,
    /// Enables retained freed-handle diagnostics.
    pub(super) freed_handle_diagnostics: bool,
    /// The profile the module was checked under (§109.1 rule 2). The
    /// runner reads the §109.5 defaults from it.
    pub(super) profile: Profile,
    /// Sets the Context interrupt flag from a second thread after this
    /// many milliseconds (§109.7). `None` starts no thread.
    pub(super) interrupt_after_millis: Option<u64>,
    /// Where this runner stores the interrupt handle of the run's
    /// Context, before the first script call (§109.4 rule 1).
    pub(super) interrupt_handle: Option<&'a OnceLock<Arc<Interrupt>>>,
    /// The limits the host set, each replacing the profile's default
    /// for that limit (§109.5).
    pub(super) limits: HostLimits,
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
    options: EntryOptions<'_>,
    write_through: Option<File>,
) -> EntryOutcome {
    let EntryOptions {
        fail_alloc_after,
        freed_handle_diagnostics,
        profile,
        interrupt_after_millis,
        interrupt_handle,
        limits,
    } = options;
    let init_ptr = module.get_finalized_function(lowered.init);
    let main = match lowered.main_id() {
        Ok(main) => main,
        Err(message) => {
            return EntryOutcome {
                run: Err(RunError::Internal(message)),
                interrupt_latency: None,
            };
        }
    };
    let main_ptr = module.get_finalized_function(main);

    let needs_panic_stdout_fallback = write_through.is_none();
    let mut ctx = Context::new();
    let mut stdout = Box::new(CapturedStdout {
        bytes: Vec::new(),
        write_through,
    });
    ctx.set_print_observer(
        Some(capture_stdout_line),
        (&mut *stdout as *mut CapturedStdout).cast::<c_void>(),
    );
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
    // §109.5: the runner applies the run-time limits before the first
    // `enter_script`, so the module initializer already runs under them.
    crate::apply_run_limits(&mut ctx, profile, limits);
    // §109.4 rule 1: the handle is obtained on the owner thread, before
    // the first script call, and it owns the cell with the Context.
    if let Some(sink) = interrupt_handle {
        let _ = sink.set(ctx.interrupt_handle());
    }
    let interrupter = interrupt_after_millis.map(|millis| {
        let handle = ctx.interrupt_handle();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(millis));
            handle.set();
            Instant::now()
        })
    });
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

    // §109.7: the recorded number is the time from the flag store to this
    // runner's return.
    let interrupt_latency = interrupter.map(|handle| {
        let store = handle.join().unwrap_or_else(|_| Instant::now());
        store.elapsed()
    });
    let trap = ctx.trap_record().map(|r| {
        let pos = lowered
            .positions
            .get(r.pos_id as usize)
            .cloned()
            .unwrap_or_else(|| Pos::new(String::new(), 0, 0));
        (r.kind, r.message.clone(), pos)
    });
    ctx.set_print_observer(None, std::ptr::null_mut());
    drop(aborting_stdout);
    let stdout = stdout.bytes;
    EntryOutcome {
        run: match trap {
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
        },
        interrupt_latency,
    }
}

#[cfg(unix)]
fn write_protocol_bytes(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(&(bytes.len() as u64).to_le_bytes())?;
    file.write_all(bytes)
}

#[cfg(unix)]
fn write_child_protocol(
    protocol: &mut File,
    outcome: &Result<CompletedRun, RunError>,
) -> std::io::Result<()> {
    match outcome {
        Ok(_) => protocol.write_all(&[0])?,
        Err(RunError::Trap(report)) => {
            protocol.write_all(&[1])?;
            protocol.write_all(&(report.rule as u32).to_le_bytes())?;
            protocol.write_all(&report.pos.line.to_le_bytes())?;
            protocol.write_all(&report.pos.col.to_le_bytes())?;
            write_protocol_bytes(protocol, report.pos.file.as_bytes())?;
            write_protocol_bytes(protocol, report.message.as_bytes())?;
        }
        Err(error) => {
            protocol.write_all(&[2])?;
            write_protocol_bytes(protocol, error.to_string().as_bytes())?;
        }
    }
    protocol.flush()
}

#[cfg(unix)]
struct ProtocolReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

#[cfg(unix)]
impl<'a> ProtocolReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], RunError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| RunError::Internal(internal("overflow reading JIT child protocol")))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| RunError::Internal(internal("truncated JIT child protocol")))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, RunError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, RunError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four protocol bytes"),
        ))
    }

    fn bytes(&mut self) -> Result<&'a [u8], RunError> {
        let len = u64::from_le_bytes(self.take(8)?.try_into().expect("eight protocol bytes"));
        let len = usize::try_from(len)
            .map_err(|_| RunError::Internal(internal("oversized field in JIT child protocol")))?;
        self.take(len)
    }

    fn string(&mut self) -> Result<String, RunError> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|error| {
            RunError::Internal(internal(format!(
                "invalid UTF-8 in JIT child protocol: {error}"
            )))
        })
    }
}

#[cfg(unix)]
fn parse_child_protocol(bytes: &[u8], stdout: Vec<u8>) -> Result<Vec<u8>, RunError> {
    let mut protocol = ProtocolReader::new(bytes);
    match protocol.u8()? {
        0 => Ok(stdout),
        1 => {
            let rule_number = protocol.u32()?;
            let rule = TrapKind::from_u32(rule_number).ok_or_else(|| {
                RunError::Internal(internal(format!(
                    "unknown trap kind {rule_number} in JIT child protocol"
                )))
            })?;
            let line = protocol.u32()?;
            let col = protocol.u32()?;
            let file = protocol.string()?;
            let message = protocol.string()?;
            Err(RunError::Trap(TrapReport {
                rule,
                message,
                pos: Pos::new(file, line, col),
                stdout,
            }))
        }
        2 => Err(RunError::Internal(protocol.string()?)),
        tag => Err(RunError::Internal(internal(format!(
            "unknown JIT child protocol tag {tag}"
        )))),
    }
}

#[cfg(unix)]
pub(super) fn execute_entry_retained(
    module: &JITModule,
    lowered: &Lowered,
    mut options: EntryOptions<'_>,
) -> Result<Vec<u8>, RunError> {
    // The child's Context is in another process, so the parent reads
    // nothing a child stores (§109.4 rule 1).
    options.interrupt_handle = None;
    let mut output = RetainedOutput::new()?;
    let writer = output.writer()?;
    let mut protocol = TemporaryFile::new("protocol")?;
    let mut stderr = TemporaryFile::new("stderr")?;

    // SAFETY: the child inherits finalized JIT code, the Context runtime, and
    // caller-supplied native symbol addresses. It reports through files and
    // calls `_exit`, while the parent alone frees the module after `waitpid`.
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
        let written = write_child_protocol(
            protocol.file.as_mut().expect("live JIT child protocol"),
            &outcome.run,
        )
        .is_ok();
        // SAFETY: this is the forked child. `_exit` avoids running inherited
        // parent destructors or freeing the parent's JIT mappings.
        unsafe { libc::_exit(if written { 0 } else { 121 }) };
    }
    drop(writer);

    let mut status = 0;
    loop {
        // SAFETY: `child` is the positive PID returned by `fork`; `status` is
        // live writable storage and this parent waits for that child only.
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
    parse_child_protocol(&protocol, stdout)
}

#[cfg(not(unix))]
pub(super) fn execute_entry_retained(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions<'_>,
) -> Result<Vec<u8>, RunError> {
    let output = RetainedOutput::new()?;
    let writer = output.writer()?;
    execute_entry(module, lowered, options, Some(writer))
        .run
        .map(|run| run.stdout)
}

pub(super) fn run_entry(
    module: &JITModule,
    lowered: &Lowered,
    options: EntryOptions<'_>,
) -> Result<(Vec<u8>, Duration), RunError> {
    execute_entry(module, lowered, options, None)
        .run
        .map(|run| (run.stdout, run.elapsed))
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
