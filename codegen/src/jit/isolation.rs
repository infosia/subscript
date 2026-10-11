//! The process that runs a dev-tier program on Unix (compiler.md §190).
//!
//! A single-threaded caller forks (rule 1). A multithreaded caller
//! starts a runner process with `posix_spawn` (rule 2), except for a run
//! with a native library or a file provider, which it refuses (rule 3).
//! The runner is the caller's own executable (rule 5): a constructor of
//! this library sees the marker variable and argument before `main` and
//! runs the runner. The runner exists on Apple targets and on Linux with
//! glibc, whose loader passes `argc` and `argv` to a constructor. The runner compiles and runs the program and reports in the
//! form of a forked child (`protocol.rs`).

use std::time::{Duration, Instant};

use subscript_compiler::SourceFile;

use super::RunError;
use crate::lower::internal;
use crate::{RunConfig, RunOutput};

/// The reason a `fork` did not happen.
pub(super) const RULE_1_MESSAGE: &str =
    "compiler.md §190.1 rule 1: the dev tier forks only a single-threaded process";

/// The reason a multithreaded run with a native library or a file
/// provider did not run.
pub(super) const RULE_3_MESSAGE: &str = "compiler.md §190.1 rule 3: a dev run with a native \
    library or a file provider needs a single-threaded caller; run it on the main thread of a \
    process that has no other thread";

/// The number of threads in this process, or `None` if the platform
/// does not give it.
#[cfg(target_vendor = "apple")]
pub(super) fn thread_count() -> Option<usize> {
    extern "C" {
        static mach_task_self_: libc::mach_port_t;
        fn mach_port_deallocate(
            task: libc::mach_port_t,
            name: libc::mach_port_t,
        ) -> libc::kern_return_t;
    }
    // SAFETY: `mach_task_self_` is the task port of this process, set
    // before `main` and never written after.
    let task = unsafe { mach_task_self_ };
    let mut list: libc::thread_act_array_t = std::ptr::null_mut();
    let mut count: libc::mach_msg_type_number_t = 0;
    // SAFETY: both out-pointers are live storage of the declared types.
    let status = unsafe { libc::task_threads(task, &mut list, &mut count) };
    if status != 0 {
        return None;
    }
    let len = count as usize;
    for index in 0..len {
        // SAFETY: `task_threads` returned `count` thread ports in `list`;
        // this releases the send right that it gave for each one.
        unsafe { mach_port_deallocate(task, *list.add(index)) };
    }
    // SAFETY: `task_threads` allocated `list` in this task with room for
    // `count` ports; nothing reads it after this call.
    unsafe {
        libc::vm_deallocate(
            task,
            list as libc::vm_address_t,
            len * std::mem::size_of::<libc::thread_act_t>(),
        )
    };
    Some(len)
}

/// The number of threads in this process, or `None` if the platform
/// does not give it.
#[cfg(target_os = "linux")]
pub(super) fn thread_count() -> Option<usize> {
    std::fs::read_dir("/proc/self/task")
        .ok()
        .map(|tasks| tasks.filter(Result::is_ok).count())
}

/// The number of threads in this process, or `None` if the platform
/// does not give it.
#[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
pub(super) fn thread_count() -> Option<usize> {
    None
}

/// The thread count as a diagnostic shows it.
pub(super) fn describe_count(threads: Option<usize>) -> String {
    threads.map_or_else(|| "an unknown number of".to_string(), |n| n.to_string())
}

/// The thread count of a run. If the run has a native library or a file
/// provider (`settle`) and the count is above 1, the count is read again
/// for up to 2 ms (compiler.md §190.1 rule 3): the platform counts a
/// joined thread for some microseconds after the join. A run without
/// such inputs reads the count once.
pub(super) fn thread_count_for_run(settle: bool) -> Option<usize> {
    let mut count = thread_count();
    if !settle || count == Some(1) || count.is_none() {
        return count;
    }
    let deadline = Instant::now() + Duration::from_millis(2);
    while count != Some(1) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_micros(20));
        count = thread_count();
    }
    count
}

/// Runs `files` from a multithreaded caller (compiler.md §190.1 rules 2
/// to 4). The parent checks the program; the runner compiles and runs
/// it.
pub(super) fn run_multithreaded(
    files: &[SourceFile],
    config: RunConfig<'_>,
    threads: Option<usize>,
) -> Result<RunOutput, RunError> {
    let check = config.check_options();
    let RunConfig {
        enabled_modules: _,
        file_provider,
        native_libraries,
        fail_alloc_after,
        freed_handle_diagnostics,
        memory_accounting: _,
        pre_init_hook: _,
        pre_entry_hook: _,
        post_run_hook: _,
    } = config;
    let hir = subscript_compiler::check_program_with(files, &check).map_err(RunError::Rejected)?;
    hir.runner_main()
        .map_err(|diagnostic| RunError::Rejected(vec![diagnostic]))?;
    if !native_libraries.is_empty() || file_provider.is_some() {
        return Err(RunError::Internal(internal(format!(
            "{RULE_3_MESSAGE} (the process has {} threads)",
            describe_count(threads)
        ))));
    }
    #[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
    {
        let request = super::protocol::RunnerRequest {
            files: std::borrow::Cow::Borrowed(files),
            check,
            fail_alloc_after,
            freed_handle_diagnostics,
        };
        Ok(RunOutput {
            stdout: runner::spawn_runner(&request)?,
            memory_accounting: None,
        })
    }
    #[cfg(not(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu"))))]
    {
        let _ = (check, fail_alloc_after, freed_handle_diagnostics);
        Err(RunError::Internal(internal(
            "compiler.md §190.1 rule 5: this platform has no dev-tier runner",
        )))
    }
}

/// The runner (compiler.md §190.1 rules 2 and 5): the spawn path of the
/// parent, the constructor that turns a new process of the same
/// executable into the runner, and the runner's work.
#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
mod runner {
    use std::ffi::{CStr, CString, OsStr, OsString};
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::super::compile::compile_jit_with;
    use super::super::entry::{collect_child, execute_entry, EntryOptions};
    use super::super::output::{RetainedOutput, TemporaryFile};
    use super::super::protocol::{write_outcome, RunnerRequest};
    use super::super::RunError;
    use crate::lower::internal;

    /// The marker variable of a runner process.
    const RUNNER_MARKER: &CStr = c"SUBSCRIPT_DEV_JIT_RUNNER";

    /// The first argument of a runner process.
    const RUNNER_SENTINEL: &CStr = c"--subscript-dev-jit-runner";

    /// The executable that a runner process starts from.
    #[cfg(target_os = "linux")]
    fn runner_executable() -> Result<std::path::PathBuf, std::io::Error> {
        Ok(std::path::PathBuf::from("/proc/self/exe"))
    }

    /// The executable that a runner process starts from.
    #[cfg(target_vendor = "apple")]
    fn runner_executable() -> Result<std::path::PathBuf, std::io::Error> {
        std::env::current_exe()
    }

    /// The loader calls this before `main`, with `argc` and `argv`. It
    /// returns at once unless the marker variable is set and `argv[1]`
    /// is the sentinel. A runner removes the marker, so a process that
    /// the program starts does not inherit it.
    extern "C" fn run_if_marked(
        argc: libc::c_int,
        argv: *const *const libc::c_char,
        _envp: *const *const libc::c_char,
    ) {
        let count = usize::try_from(argc).unwrap_or(0);
        // SAFETY: the loader passes `argc` live NUL-terminated arguments
        // in `argv`; the marker name is NUL-terminated; before `main` no
        // other thread reads or changes the environment.
        unsafe {
            if count < 2
                || libc::getenv(RUNNER_MARKER.as_ptr()).is_null()
                || CStr::from_ptr(*argv.add(1)) != RUNNER_SENTINEL
            {
                return;
            }
            libc::unsetenv(RUNNER_MARKER.as_ptr());
        }
        let args: Vec<OsString> = (2..count)
            .map(|index| {
                // SAFETY: as above, `index` is below `argc`.
                let arg = unsafe { CStr::from_ptr(*argv.add(index)) };
                OsStr::from_bytes(arg.to_bytes()).to_os_string()
            })
            .collect();
        let code = runner_main(&args);
        // SAFETY: a runner process ends here; `main` of the executable
        // never runs.
        unsafe { libc::_exit(code.into()) }
    }

    /// The loader runs each pointer of this section before `main`. The
    /// spawn path names this static, so every executable that can start
    /// a runner links the object that holds it.
    #[used]
    #[cfg_attr(target_vendor = "apple", link_section = "__DATA,__mod_init_func")]
    #[cfg_attr(target_os = "linux", link_section = ".init_array")]
    static RUNNER_CONSTRUCTOR: extern "C" fn(
        libc::c_int,
        *const *const libc::c_char,
        *const *const libc::c_char,
    ) = run_if_marked;

    fn c_string(bytes: &[u8], what: &str) -> Result<CString, RunError> {
        CString::new(bytes).map_err(|_| {
            RunError::Internal(internal(format!("the JIT runner {what} holds a NUL byte")))
        })
    }

    fn path_c_string(path: &Path, what: &str) -> Result<CString, RunError> {
        c_string(path.as_os_str().as_bytes(), what)
    }

    fn spawn_error(what: &str, status: libc::c_int) -> RunError {
        RunError::Internal(internal(format!(
            "{what}: {}",
            std::io::Error::from_raw_os_error(status)
        )))
    }

    /// Starts the current executable as a runner with `posix_spawn`,
    /// waits for it, and returns its retained stdout or its outcome.
    pub(super) fn spawn_runner(request: &RunnerRequest) -> Result<Vec<u8>, RunError> {
        // The constructor object must be in this executable (rule 5).
        std::hint::black_box(&RUNNER_CONSTRUCTOR);
        let runner = runner_executable().map_err(|error| {
            RunError::Internal(internal(format!(
                "compiler.md §190.1 rule 5: the current executable is unknown: {error}"
            )))
        })?;
        let mut output = RetainedOutput::new()?;
        let mut request_file = TemporaryFile::new("request")?;
        let mut protocol = TemporaryFile::new("protocol")?;
        let mut stderr = TemporaryFile::new("stderr")?;
        {
            let file = request_file.file.as_mut().expect("live JIT runner request");
            file.write_all(&request.encode())
                .and_then(|()| file.flush())
                .map_err(|error| {
                    RunError::Internal(internal(format!("write JIT runner request: {error}")))
                })?;
        }

        let runner_path = path_c_string(&runner, "path")?;
        let args = [
            runner_path.clone(),
            RUNNER_SENTINEL.to_owned(),
            path_c_string(request_file.path(), "request path")?,
            path_c_string(output.path(), "output path")?,
            path_c_string(protocol.path(), "protocol path")?,
        ];
        let mut argv: Vec<*mut libc::c_char> =
            args.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
        argv.push(std::ptr::null_mut());
        let environment = std::env::vars_os()
            .filter(|(key, _)| key.as_bytes() != RUNNER_MARKER.to_bytes())
            .map(|(key, value)| {
                let mut pair = key.as_bytes().to_vec();
                pair.push(b'=');
                pair.extend_from_slice(value.as_bytes());
                c_string(&pair, "environment")
            })
            .chain(std::iter::once(c_string(
                &[RUNNER_MARKER.to_bytes(), b"=1"].concat(),
                "marker",
            )))
            .collect::<Result<Vec<_>, _>>()?;
        let mut envp: Vec<*mut libc::c_char> = environment
            .iter()
            .map(|pair| pair.as_ptr().cast_mut())
            .collect();
        envp.push(std::ptr::null_mut());
        let stderr_path = path_c_string(stderr.path(), "stderr path")?;

        let mut actions = std::mem::MaybeUninit::<libc::posix_spawn_file_actions_t>::uninit();
        // SAFETY: `actions` is storage for one file-actions object.
        let status = unsafe { libc::posix_spawn_file_actions_init(actions.as_mut_ptr()) };
        if status != 0 {
            return Err(spawn_error("initialize JIT runner file actions", status));
        }
        // SAFETY: `actions` was initialized above and stays live until the
        // destroy call below. Every pointer argument is a live
        // NUL-terminated string or a NULL-terminated array of them.
        let spawned = unsafe {
            let actions = actions.as_mut_ptr();
            let mut status = libc::posix_spawn_file_actions_addopen(
                actions,
                libc::STDERR_FILENO,
                stderr_path.as_ptr(),
                libc::O_WRONLY | libc::O_APPEND,
                0,
            );
            let mut child: libc::pid_t = 0;
            if status == 0 {
                status = libc::posix_spawn(
                    &mut child,
                    runner_path.as_ptr(),
                    actions,
                    std::ptr::null(),
                    argv.as_ptr(),
                    envp.as_ptr(),
                );
            }
            libc::posix_spawn_file_actions_destroy(actions);
            if status == 0 {
                Ok(child)
            } else {
                Err(status)
            }
        };
        let child = spawned.map_err(|status| {
            spawn_error(&format!("start JIT runner `{}`", runner.display()), status)
        })?;
        collect_child(child, &mut output, &mut stderr, &mut protocol)
    }

    /// The runner's work: it reads a request, compiles and runs the
    /// program, writes the outcome, and ends the process with `_exit`.
    /// `args` are the request path, the output path, and the protocol
    /// path. It returns only for a usage error, with the exit code.
    pub(super) fn runner_main(args: &[OsString]) -> u8 {
        let [request, output, protocol] = args else {
            eprintln!("dev-tier runner: expected <request> <output> <protocol>");
            return 2;
        };
        let mut protocol = match OpenOptions::new().write(true).open(protocol) {
            Ok(file) => file,
            Err(error) => {
                eprintln!(
                    "dev-tier runner: open {}: {error}",
                    Path::new(protocol).display()
                );
                return 121;
            }
        };
        let outcome = run_request(request, output);
        let written = write_outcome(&mut protocol, &outcome).is_ok();
        // SAFETY: the run is over and its outcome is in the protocol
        // file. `_exit` ends the process as the forked child ends,
        // without destructors of the run's state.
        unsafe { libc::_exit(if written { 0 } else { 121 }) }
    }

    fn run_request(request: &OsStr, output: &OsStr) -> Result<(), RunError> {
        let bytes = std::fs::read(request).map_err(|error| {
            RunError::Internal(internal(format!("read JIT runner request: {error}")))
        })?;
        let request = RunnerRequest::decode(&bytes)?;
        let (module, lowered) = compile_jit_with(&request.files, &[], &request.check)?;
        let writer: File = OpenOptions::new()
            .append(true)
            .open(output)
            .map_err(|error| {
                RunError::Internal(internal(format!("open JIT output file: {error}")))
            })?;
        let options = EntryOptions {
            file_provider: None,
            fail_alloc_after: request.fail_alloc_after,
            freed_handle_diagnostics: request.freed_handle_diagnostics,
        };
        execute_entry(&module, &lowered, options, Some(writer)).map(|_| ())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use subscript_compiler::SourceFile;

        #[test]
        fn a_runner_usage_error_returns_code_2() {
            assert_eq!(runner_main(&[]), 2);
        }

        /// §190.2 acceptance 3: a run from a libtest thread starts this
        /// test binary again as the runner. No other runner binary
        /// exists.
        #[test]
        fn a_libtest_thread_run_reaches_the_runner_in_this_binary() {
            assert_ne!(
                super::super::thread_count(),
                Some(1),
                "libtest runs this on its own thread"
            );
            let files = [SourceFile::entry(
                "main.ts",
                "export function main(): void {\n  print(`runner`);\n}\n",
            )];
            assert_eq!(crate::run_jit(&files).expect("the runner run"), b"runner\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thread_count_sees_a_live_thread() {
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let _ = wait.recv();
        });
        let during = thread_count();
        stop.send(()).expect("stop the thread");
        thread.join().expect("join the thread");
        // A platform without a count gives `None`, which is not 1.
        if let Some(during) = during {
            assert!(during >= 2, "during {during}");
        }
    }
}
