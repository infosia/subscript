//! A test main that runs the tests of a binary in two phases
//! (compiler.md §190.1 rule 3).
//!
//! A binary that runs dev-tier programs with a native library or a file
//! provider uses it (`harness = false`). Such a run needs a
//! single-threaded process. A phase 1 test starts no such run and runs
//! on a thread of the harness. A phase 2 test runs in a new process of
//! this binary (`--exact <name>`), on that process's main thread. Both
//! phases share the thread limit and overlap; the harness joins every
//! thread and waits for every child. The binary lists each test in its
//! phase. A phase 1 test that starts such a run gets the rule 3 error,
//! so a wrong list fails. A binary that carries `#![deny(dead_code)]`
//! fails to build if a test function is not listed. A non-Unix host
//! runs every test as phase 1.
//!
//! The arguments are a subset of the libtest arguments: name filters,
//! `--exact`, `--skip <name>`, `--list`, `--ignored`, and
//! `--test-threads <n>` (or `RUST_TEST_THREADS`). Other flags are
//! accepted and have no effect. The output keeps the libtest
//! `test <name> ... ok` and `test result:` lines.

use std::io::Write;
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// The phase of a test.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The test starts no dev run with a native library or a file
    /// provider. It runs in parallel with the other phase 1 tests.
    Parallel,
    /// The test starts such a run. It runs on the main thread after
    /// phase 1.
    MainThread,
}

/// One test of a binary.
pub struct Test {
    /// The name that filters match and the output shows: the function
    /// path in the binary, as libtest shows it.
    pub name: String,
    /// The test body. A panic is a failure.
    pub run: fn(),
    /// The phase that runs the test.
    pub phase: Phase,
}

/// Lists test functions as [`Test`] values in their phases:
/// `parallel: [..], main_thread: [..]`. A name is a function path, such
/// as `operations::a_test`. An attribute before a name, such as
/// `#[cfg(unix)]`, applies to that entry.
#[macro_export]
macro_rules! main_thread_tests {
    (
        parallel: [$($(#[$pmeta:meta])* $($pseg:ident)::+),* $(,)?],
        main_thread: [$($(#[$mmeta:meta])* $($mseg:ident)::+),* $(,)?] $(,)?
    ) => {{
        // A `cfg` attribute can apply to a statement but not to an
        // element of a `vec!` list, so the entries are pushed one by one.
        #[allow(clippy::vec_init_then_push)]
        fn listed_tests() -> Vec<$crate::main_thread::Test> {
            let mut tests: Vec<$crate::main_thread::Test> = Vec::new();
            $(
                $(#[$pmeta])*
                tests.push($crate::main_thread::Test {
                    name: [$(stringify!($pseg)),+].join("::"),
                    run: $($pseg)::+,
                    phase: $crate::main_thread::Phase::Parallel,
                });
            )*
            $(
                $(#[$mmeta])*
                tests.push($crate::main_thread::Test {
                    name: [$(stringify!($mseg)),+].join("::"),
                    run: $($mseg)::+,
                    phase: $crate::main_thread::Phase::MainThread,
                });
            )*
            tests
        }
        listed_tests()
    }};
}

/// The number of phase 1 threads: `--test-threads`, then
/// `RUST_TEST_THREADS`, then the available parallelism.
fn thread_limit(argument: Option<usize>) -> usize {
    argument
        .or_else(|| {
            std::env::var("RUST_TEST_THREADS")
                .ok()
                .and_then(|value| value.parse().ok())
        })
        .or_else(|| std::thread::available_parallelism().ok().map(Into::into))
        .unwrap_or(1)
        .max(1)
}

/// The environment variable that marks a phase 2 child process.
const CHILD_ENV: &str = "SUBSCRIPT_TEST_PHASE_2_CHILD";

/// Runs `run` for each selected test on at most `limit` worker threads,
/// prints one libtest line for each test, and returns the names of the
/// failed tests in list order. The scope joins every worker.
fn run_bounded<'t>(
    tests: &[&'t Test],
    limit: usize,
    run: impl for<'s, 'e> Fn(&'t Test, &'s std::thread::Scope<'s, 'e>) -> bool + Sync,
) -> Vec<&'t str> {
    let next = AtomicUsize::new(0);
    let failed: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..limit.min(tests.len()) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(test) = tests.get(index) else {
                    break;
                };
                let passed = run(test, scope);
                println!(
                    "test {} ... {}",
                    test.name,
                    if passed { "ok" } else { "FAILED" }
                );
                if !passed {
                    failed
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(index);
                }
            });
        }
    });
    let mut failed = failed
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    failed.sort_unstable();
    failed
        .iter()
        .map(|&index| tests[index].name.as_str())
        .collect()
}

/// Runs the selected tests in their phases and returns the process exit
/// code.
pub fn run(tests: &[Test]) -> ExitCode {
    let mut exact = false;
    let mut list = false;
    let mut ignored_only = false;
    let mut threads: Option<usize> = None;
    let mut filters: Vec<String> = Vec::new();
    let mut skips: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exact" => exact = true,
            "--list" => list = true,
            "--ignored" => ignored_only = true,
            "--skip" => skips.extend(args.next()),
            "--test-threads" => threads = args.next().and_then(|n| n.parse().ok()),
            "--format" | "--color" | "-Z" | "--logfile" => {
                let _ = args.next();
            }
            flag if flag.starts_with("--test-threads=") => {
                threads = flag["--test-threads=".len()..].parse().ok();
            }
            flag if flag.starts_with('-') => {}
            filter => filters.push(filter.to_string()),
        }
    }
    let matches = |name: &str, pattern: &str| {
        if exact {
            name == pattern
        } else {
            name.contains(pattern)
        }
    };
    let selected: Vec<&Test> = tests
        .iter()
        .filter(|_| !ignored_only)
        .filter(|test| filters.is_empty() || filters.iter().any(|f| matches(&test.name, f)))
        .filter(|test| !skips.iter().any(|s| matches(&test.name, s)))
        .collect();
    let filtered_out = tests.len() - selected.len();
    if list {
        for test in &selected {
            println!("{}: test", test.name);
        }
        println!();
        println!("{} tests, 0 benchmarks", selected.len());
        return ExitCode::SUCCESS;
    }

    let start = Instant::now();
    println!();
    println!("running {} tests", selected.len());
    let mut failed: Vec<&str> = Vec::new();

    if std::env::var_os(CHILD_ENV).is_some() {
        // A phase 2 child: its one selected test runs on the main thread
        // of this single-threaded process.
        let [test] = selected.as_slice() else {
            println!(
                "a phase 2 child selected {} tests; it runs exactly 1",
                selected.len()
            );
            return ExitCode::from(101);
        };
        print!("test {} ... ", test.name);
        let _ = std::io::stdout().flush();
        if std::panic::catch_unwind(test.run).is_ok() {
            println!("ok");
        } else {
            println!("FAILED");
            failed.push(&test.name);
        }
    } else {
        // Both phases share one bounded set of workers, so they overlap.
        // A phase 1 test runs on its own thread with the test name, as
        // under libtest. A phase 2 test runs in a new process of this
        // binary, on its main thread, and its worker waits for it. A
        // non-Unix host runs every test as phase 1 (§190.1 rule 3).
        let executable = std::env::current_exe().ok();
        failed.extend(run_bounded(
            &selected,
            thread_limit(threads),
            |test, scope| {
                if test.phase == Phase::Parallel || cfg!(not(unix)) {
                    return match std::thread::Builder::new()
                        .name(test.name.clone())
                        .spawn_scoped(scope, test.run)
                    {
                        Ok(thread) => thread.join().is_ok(),
                        Err(error) => {
                            println!("{}: start the test thread: {error}", test.name);
                            false
                        }
                    };
                }
                let Some(executable) = &executable else {
                    println!("{}: the current executable is unknown", test.name);
                    return false;
                };
                match Command::new(executable)
                    .args(["--exact", &test.name])
                    .env(CHILD_ENV, "1")
                    .output()
                {
                    Ok(output) if output.status.success() => true,
                    Ok(output) => {
                        println!(
                            "---- {} ({}) ----\n{}{}",
                            test.name,
                            output.status,
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr)
                        );
                        false
                    }
                    Err(error) => {
                        println!("{}: start the test process: {error}", test.name);
                        false
                    }
                }
            },
        ));
    }

    let passed = selected.len() - failed.len();
    if !failed.is_empty() {
        println!();
        println!("failures:");
        for name in &failed {
            println!("    {name}");
        }
    }
    println!();
    println!(
        "test result: {}. {passed} passed; {} failed; 0 ignored; 0 measured; {filtered_out} filtered out; finished in {:.2}s",
        if failed.is_empty() { "ok" } else { "FAILED" },
        failed.len(),
        start.elapsed().as_secs_f64()
    );
    println!();
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(101)
    }
}
