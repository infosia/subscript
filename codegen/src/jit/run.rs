//! The dev-tier run entry points the crate exports.
//!
//! Every entry point here returns the stdout bytes of one run. Under the
//! sandbox profile those bytes come from the Context sink, which the
//! quota charges, because the runner installs no print observer
//! (`specs/blocks/compiler.md` §109.7a).

use std::time::Duration;

use subscript_compiler::SourceFile;

use super::compile::compile_jit;
use super::entry::{execute_entry, execute_entry_retained, memory_accounting, EntryOptions};
use super::{JitMemoryAccounting, RunError};
use crate::lower::internal;
use crate::{NativeLibrary, RunConfig, RunOutput};

/// Checks `files`, lowers the typed HIR through the shared CLIF
/// lowering, executes the exported `main(): void` under the dev JIT,
/// and returns the exact stdout bytes the run produced.
///
/// On Unix, the program runs in a forked child so that output survives a run
/// that does not complete normally. Consequently, side effects that a native
/// library performs on host memory during the run are not observable in the
/// caller's process. On non-Unix platforms, the program runs in-process.
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when the run trapped (rule + message + TS
/// position + pre-trap stdout),
/// [`RunError::UnresolvedForeignSymbol`] when the program calls a symbol
/// but no native library was supplied, [`RunError::AbnormalTermination`]
/// when generated or foreign code ends outside the trap protocol, and
/// [`RunError::Internal`] on backend failures.
pub fn run_jit(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    Ok(run_jit_configured(files, RunConfig::default())?.stdout)
}

/// Runs the development tier with one complete option record.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit`]. Host hooks are
/// shipping-tier options and produce [`RunError::Internal`] here.
pub fn run_jit_configured(
    files: &[SourceFile],
    config: RunConfig<'_>,
) -> Result<RunOutput, RunError> {
    if config.pre_entry_hook.is_some() || config.post_run_hook.is_some() {
        return Err(RunError::Internal(internal(
            "host hooks are not available in the development tier",
        )));
    }
    let (module, lowered, profile) = compile_jit(files, config.native_libraries, config.profile)?;
    let options = EntryOptions {
        fail_alloc_after: config.fail_alloc_after,
        freed_handle_diagnostics: config.freed_handle_diagnostics,
        profile,
        interrupt_after_millis: config.interrupt_after_millis,
        interrupt_handle: config.interrupt_handle,
        limits: config.host_limits(),
    };
    let outcome = if config.memory_accounting {
        execute_entry(&module, &lowered, options, None)
            .run
            .map(|run| RunOutput {
                memory_accounting: Some(memory_accounting(&run.ctx)),
                stdout: run.stdout,
            })
    } else {
        execute_entry_retained(&module, &lowered, options).map(|stdout| RunOutput {
            stdout,
            memory_accounting: None,
        })
    };
    // SAFETY: all executions above returned and no pointer into JIT memory survives.
    unsafe { module.free_memory() };
    outcome
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
    run_jit_with_memory_accounting_and_native_libraries(files, &[], freed_handle_diagnostics)
}

/// Runs the dev JIT with native libraries and returns stdout plus the live
/// Context's post-run allocation accounting.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit_with_native_libraries`].
pub fn run_jit_with_memory_accounting_and_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
    freed_handle_diagnostics: bool,
) -> Result<(Vec<u8>, JitMemoryAccounting), RunError> {
    let output = run_jit_configured(
        files,
        RunConfig {
            native_libraries: libraries,
            freed_handle_diagnostics,
            memory_accounting: true,
            ..RunConfig::default()
        },
    )?;
    let accounting = output.memory_accounting.ok_or_else(|| {
        RunError::Internal(internal("configured accounting run returned no accounting"))
    })?;
    Ok((output.stdout, accounting))
}

/// Checks, lowers, and runs `files` through the dev JIT with the
/// caller-supplied native libraries available for foreign calls.
///
/// On Unix, the program runs in a forked child so that output survives a run
/// that does not complete normally. Consequently, side effects that a native
/// library performs on host memory during the run are not observable in the
/// caller's process. On non-Unix platforms, the program runs in-process.
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
    Ok(run_jit_configured(
        files,
        RunConfig {
            native_libraries: libraries,
            ..RunConfig::default()
        },
    )?
    .stdout)
}

/// Runs the dev JIT with freed-handle diagnostics enabled and the
/// caller-supplied native libraries available for foreign calls.
///
/// The mode uses threshold 0 and the recommended 1 GiB retention budget,
/// matching [`run_jit_with_memory_accounting`] when its diagnostics argument
/// is true.
///
/// On Unix, the program runs in a forked child so that output survives a run
/// that does not complete normally. Consequently, side effects that a native
/// library performs on host memory during the run are not observable in the
/// caller's process. On non-Unix platforms, the program runs in-process.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit_with_native_libraries`].
pub fn run_jit_with_freed_handle_diagnostics_and_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    Ok(run_jit_configured(
        files,
        RunConfig {
            native_libraries: libraries,
            freed_handle_diagnostics: true,
            ..RunConfig::default()
        },
    )?
    .stdout)
}

/// Runs the dev tier while refusing the `n`-th object-level Context
/// allocation after Context creation.
///
/// The injected fault is armed before `subscript_init`, so module-initializer
/// allocations are part of the count.
///
/// On Unix, the program runs in a forked child so that output survives a run
/// that does not complete normally. Consequently, side effects that a native
/// library performs on host memory during the run are not observable in the
/// caller's process. On non-Unix platforms, the program runs in-process.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_jit`].
pub fn run_jit_with_alloc_failure(files: &[SourceFile], n: u64) -> Result<Vec<u8>, RunError> {
    Ok(run_jit_configured(
        files,
        RunConfig {
            fail_alloc_after: Some(n),
            ..RunConfig::default()
        },
    )?
    .stdout)
}

/// Runs the development tier in this process and sets the Context
/// interrupt flag from a second thread (`specs/blocks/compiler.md`
/// §109.7).
///
/// `config.interrupt_after_millis` is the delay before the flag store.
/// `None` starts no thread, so a program with an endless loop never
/// returns: that shape is the firing control, and the caller bounds it.
///
/// The run is in process, because the thread that sets the flag and the
/// Context must belong to one process. The second return value is the
/// time from the flag store to this function's return.
pub fn run_jit_interrupted(
    files: &[SourceFile],
    config: RunConfig<'_>,
) -> (Result<Vec<u8>, RunError>, Option<Duration>) {
    let compiled = compile_jit(files, config.native_libraries, config.profile);
    let (module, lowered, profile) = match compiled {
        Ok(compiled) => compiled,
        Err(error) => return (Err(error), None),
    };
    let outcome = execute_entry(
        &module,
        &lowered,
        EntryOptions {
            fail_alloc_after: config.fail_alloc_after,
            freed_handle_diagnostics: config.freed_handle_diagnostics,
            profile,
            interrupt_after_millis: config.interrupt_after_millis,
            interrupt_handle: config.interrupt_handle,
            limits: config.host_limits(),
        },
        None,
    );
    let run = outcome.run.map(|run| run.stdout);
    // SAFETY: the execution above returned and no pointer into JIT memory
    // survives.
    unsafe { module.free_memory() };
    (run, outcome.interrupt_latency)
}
