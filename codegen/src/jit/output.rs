//! Captured stdout of one dev-tier run and the files that retain it.
//!
//! A run appends every completed line to a helper-owned file, so an
//! abnormal child termination still returns those bytes to the caller.

use std::cell::Cell;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::{RunError, JIT_OUTPUT_FILE_ENV};
use crate::lower::internal;

pub(super) struct CapturedStdout {
    pub(super) bytes: Vec<u8>,
    pub(super) write_through: Option<File>,
}

pub(super) struct TemporaryFile {
    path: Option<PathBuf>,
    pub(super) file: Option<File>,
}

impl TemporaryFile {
    pub(super) fn new(tag: &str) -> Result<Self, RunError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("subscript-jit-{}-{tag}-{n}", std::process::id()));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path: Some(path),
                        file: Some(file),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(RunError::Internal(internal(format!(
                        "create JIT temporary file {}: {error}",
                        path.display()
                    ))));
                }
            }
        }
        Err(RunError::Internal(internal(
            "could not allocate a unique JIT temporary file",
        )))
    }

    #[cfg(unix)]
    pub(super) fn bytes(&mut self) -> Result<Vec<u8>, RunError> {
        let file = self.file.as_mut().expect("live temporary file");
        file.seek(SeekFrom::Start(0)).map_err(|error| {
            RunError::Internal(internal(format!("seek JIT temporary file: {error}")))
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|error| {
            RunError::Internal(internal(format!("read JIT temporary file: {error}")))
        })?;
        Ok(bytes)
    }

    fn take_parts(mut self) -> (PathBuf, File) {
        (
            self.path.take().expect("live temporary path"),
            self.file.take().expect("live temporary file"),
        )
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(super) struct RetainedOutput {
    file: File,
    #[cfg(unix)]
    start: u64,
    owned_path: Option<PathBuf>,
}

impl RetainedOutput {
    pub(super) fn new() -> Result<Self, RunError> {
        if let Some(path) = std::env::var_os(JIT_OUTPUT_FILE_ENV) {
            let path = Path::new(&path);
            let file = OpenOptions::new()
                .read(true)
                .append(true)
                .open(path)
                .map_err(|error| {
                    RunError::Internal(internal(format!(
                        "open JIT output file {}: {error}",
                        path.display()
                    )))
                })?;
            #[cfg(unix)]
            let start = file
                .metadata()
                .map_err(|error| {
                    RunError::Internal(internal(format!(
                        "inspect JIT output file {}: {error}",
                        path.display()
                    )))
                })?
                .len();
            return Ok(Self {
                file,
                #[cfg(unix)]
                start,
                owned_path: None,
            });
        }

        let (path, file) = TemporaryFile::new("output")?.take_parts();
        Ok(Self {
            file,
            #[cfg(unix)]
            start: 0,
            owned_path: Some(path),
        })
    }

    pub(super) fn writer(&self) -> Result<File, RunError> {
        self.file.try_clone().map_err(|error| {
            RunError::Internal(internal(format!("clone JIT output file: {error}")))
        })
    }

    #[cfg(unix)]
    pub(super) fn bytes(&mut self) -> Result<Vec<u8>, RunError> {
        self.file
            .seek(SeekFrom::Start(self.start))
            .map_err(|error| {
                RunError::Internal(internal(format!("seek JIT output file: {error}")))
            })?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes).map_err(|error| {
            RunError::Internal(internal(format!("read JIT output file: {error}")))
        })?;
        Ok(bytes)
    }
}

impl Drop for RetainedOutput {
    fn drop(&mut self) {
        if let Some(path) = &self.owned_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

thread_local! {
    /// The active run's captured output. A panic in generated/runtime code can
    /// abort rather than unwind across the C ABI, so the process-wide panic
    /// hook uses this pointer to surface bytes that the helper could not
    /// otherwise return.
    static ABORTING_STDOUT: Cell<*const Vec<u8>> = const { Cell::new(std::ptr::null()) };
    static ABORTING_STDOUT_FLUSHED: Cell<bool> = const { Cell::new(false) };
}

pub(super) struct AbortingStdoutGuard;

impl AbortingStdoutGuard {
    pub(super) fn install(stdout: &Vec<u8>) -> Self {
        static INSTALL_HOOK: std::sync::Once = std::sync::Once::new();
        INSTALL_HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                ABORTING_STDOUT.with(|active| {
                    let stdout = active.get();
                    if stdout.is_null() {
                        return;
                    }
                    ABORTING_STDOUT_FLUSHED.with(|flushed| {
                        if flushed.replace(true) {
                            return;
                        }
                        // SAFETY: the guard keeps the boxed Vec alive and at
                        // a stable address for the whole generated-code call.
                        let bytes = unsafe { &*stdout };
                        let mut process_stdout = std::io::stdout().lock();
                        let _ = process_stdout.write_all(bytes);
                        let _ = process_stdout.flush();
                    });
                });
                previous(info);
            }));
        });
        ABORTING_STDOUT.with(|active| {
            debug_assert!(active.get().is_null(), "nested JIT stdout guard");
            active.set(stdout);
        });
        ABORTING_STDOUT_FLUSHED.with(|flushed| flushed.set(false));
        Self
    }
}

impl Drop for AbortingStdoutGuard {
    fn drop(&mut self) {
        ABORTING_STDOUT.with(|active| active.set(std::ptr::null()));
        ABORTING_STDOUT_FLUSHED.with(|flushed| flushed.set(false));
    }
}

/// Captures each printed line outside the Context and, when configured,
/// flushes it to a parent-owned file before returning. The file survives a
/// later hard termination that cannot run Rust destructors or a panic hook.
pub(super) unsafe extern "C" fn capture_stdout_line(
    userdata: *mut c_void,
    line: *const u8,
    line_len: u64,
) {
    // SAFETY: execute_entry supplies a live Box<CapturedStdout> for the
    // duration of every generated-code call; the runtime supplies a live line
    // slice.
    let stdout = unsafe { &mut *userdata.cast::<CapturedStdout>() };
    let line = unsafe { std::slice::from_raw_parts(line, line_len as usize) };
    stdout.bytes.extend_from_slice(line);
    stdout.bytes.push(b'\n');
    if let Some(file) = &mut stdout.write_through {
        let _ = file.write_all(line);
        let _ = file.write_all(b"\n");
        let _ = file.flush();
    }
}
