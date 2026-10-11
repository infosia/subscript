//! compiler.md §190: a dev run forks only a single-threaded process.
//!
//! This binary runs its tests in the two phases of
//! `support/main_thread.rs` (`harness = false`), so a phase 2 test
//! controls the thread count of the process at each run.

// compiler.md §190.1 rule 3: a test function that the phase list does
// not name fails the build.
#![deny(dead_code)]

#[path = "support/main_thread.rs"]
mod main_thread;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use subscript_codegen::{run_jit, run_jit_with_native_libraries, NativeLibrary};
use subscript_compiler::SourceFile;

const WORKER_PROGRAM: &str = "class EchoMessage {\n\
  value: i32;\n\
  constructor(value: i32) { this.value = value; }\n\
}\n\
function echo(inbox: Inbox<EchoMessage>, outbox: Outbox<EchoMessage>): void {\n\
  const message: EchoMessage | null = inbox.wait();\n\
  if (message !== null) { outbox.post(message); }\n\
}\n\
export function main(): void {\n\
  const worker: Worker<EchoMessage, EchoMessage> = Worker.spawn(echo);\n\
  worker.post(new EchoMessage(37));\n\
  worker.close();\n\
  worker.join();\n\
  const reply: EchoMessage | null = worker.poll();\n\
  if (reply !== null) { print(`echo=${reply.value}`); }\n\
}\n";

/// Runs of the stress test. It is a regression guard: the rule 1 check
/// makes the hang unreachable. The Red evidence is in
/// `specs/tracking/jit-runner-fork.md`.
const STRESS_RUNS: usize = if cfg!(debug_assertions) { 8 } else { 50 };

/// The threads that start and join threads while the runs go on.
const CHURN_THREADS: usize = 6;

/// §190.2 acceptance 1: one Worker program runs through `run_jit` while
/// six other threads start and join threads without pause. Every run
/// completes with the program's output.
fn a_worker_run_completes_while_other_threads_start_threads() {
    let files = [SourceFile::entry("main.ts", WORKER_PROGRAM)];
    let stop = Arc::new(AtomicBool::new(false));
    let churn: Vec<_> = (0..CHURN_THREADS)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::spawn(|| {})
                        .join()
                        .expect("join an empty thread");
                }
            })
        })
        .collect();
    let outcomes: Vec<_> = (0..STRESS_RUNS).map(|_| run_jit(&files)).collect();
    stop.store(true, Ordering::Relaxed);
    for thread in churn {
        thread.join().expect("join a churn thread");
    }
    for (run, outcome) in outcomes.into_iter().enumerate() {
        match outcome {
            Ok(stdout) => assert_eq!(stdout, b"echo=37\n", "run {run}"),
            Err(error) => panic!("run {run}: {error}"),
        }
    }
}

extern "C" fn native_seven() -> i32 {
    7
}

fn native_program() -> (Vec<SourceFile>, NativeLibrary) {
    let files = vec![
        SourceFile::ambient(
            "seven.generated.d.ts",
            "// @subscript-c-header include=\"seven.h\"\n\
             declare function nativeSeven(): i32;\n",
        ),
        SourceFile::entry(
            "main.ts",
            "export function main(): void {\n  print(`${nativeSeven()}`);\n}\n",
        ),
    ];
    // SAFETY: `native_seven` has static lifetime and the C signature
    // `int32_t nativeSeven(void)` that the mirror declares.
    let library = unsafe {
        NativeLibrary::new(
            Vec::new(),
            Vec::new(),
            vec![("nativeSeven".to_string(), native_seven as *const u8)],
        )
    };
    (files, library)
}

/// §190.2 acceptance 2: a second live thread and a native library give
/// the rule 3 error. The run does not fork: the error comes before any
/// child exists, and a forked run returns the program output instead.
/// Windows runs the dev tier in process (§190.1 rule 7), so the rule
/// applies on Unix only.
#[cfg(unix)]
fn a_native_run_with_a_second_thread_is_the_rule_3_error() {
    use subscript_codegen::RunError;

    let (files, library) = native_program();
    let (stop, wait) = std::sync::mpsc::channel::<()>();
    let second = std::thread::spawn(move || {
        let _ = wait.recv();
    });
    let outcome = run_jit_with_native_libraries(&files, std::slice::from_ref(&library));
    stop.send(()).expect("stop the second thread");
    second.join().expect("join the second thread");
    match outcome {
        Err(RunError::Internal(message)) => {
            assert!(message.contains("§190.1 rule 3"), "{message}");
        }
        other => panic!("expected the rule 3 error, got {other:?}"),
    }
}

/// §190.1 rule 3: a native run listed in phase 1 runs on a harness
/// thread, so the process has more than one thread. The run returns the
/// rule 3 error, so a wrong phase list fails. The control is the phase 2
/// test below, with the same body except the expected outcome.
#[cfg(unix)]
fn a_phase_1_native_run_is_the_rule_3_error() {
    use subscript_codegen::RunError;

    let (files, library) = native_program();
    match run_jit_with_native_libraries(&files, std::slice::from_ref(&library)) {
        Err(RunError::Internal(message)) => {
            assert!(message.contains("§190.1 rule 3"), "{message}");
        }
        other => panic!("expected the rule 3 error, got {other:?}"),
    }
}

/// §190.1 rule 3: the platform counts a joined thread for some
/// microseconds after the join. A native run that starts at once after
/// several joins reads the count again and runs. The control is the
/// second-thread test above, whose live thread still gives the rule 3
/// error.
fn a_native_run_right_after_joins_runs_the_program() {
    let (files, library) = native_program();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {});
        }
    });
    let stdout = run_jit_with_native_libraries(&files, std::slice::from_ref(&library))
        .expect("the run right after the joins");
    assert_eq!(stdout, b"7\n");
}

/// The control of the rule 3 tests above: the same program and library in
/// phase 2, on the main thread of a single-threaded process, forks and
/// runs.
fn a_native_run_on_the_only_thread_runs_the_program() {
    let (files, library) = native_program();
    let stdout = run_jit_with_native_libraries(&files, std::slice::from_ref(&library))
        .expect("the single-threaded run");
    assert_eq!(stdout, b"7\n");
}

/// The root file of each `harness = false` test target of the packages
/// that hold two-phase binaries.
fn two_phase_roots() -> Vec<PathBuf> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut roots = Vec::new();
    for package in ["codegen", "cli", "examples"] {
        let directory = repository.join(package);
        let manifest =
            std::fs::read_to_string(directory.join("Cargo.toml")).expect("read the manifest");
        for target in manifest.split("[[test]]").skip(1) {
            let section = target.split("\n[").next().unwrap_or_default();
            let value = |key: &str| {
                section.lines().find_map(|line| {
                    let (name, value) = line.split_once('=')?;
                    (name.trim() == key).then(|| value.trim().trim_matches('"').to_string())
                })
            };
            if value("harness").as_deref() != Some("false") {
                continue;
            }
            let name = value("name").expect("a test target has a name");
            let path = value("path").unwrap_or_else(|| format!("tests/{name}.rs"));
            roots.push(directory.join(path));
        }
    }
    roots
}

/// `file` and every module file that it declares, followed through
/// `mod name;` and `#[path = "..."] mod name;`.
fn module_files(file: &Path, is_root: bool, out: &mut Vec<PathBuf>) {
    let source = std::fs::read_to_string(file)
        .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
    out.push(file.to_path_buf());
    let directory = file.parent().expect("a source file has a directory");
    let owns_directory = is_root
        || matches!(
            file.file_name().and_then(|name| name.to_str()),
            Some("mod.rs" | "main.rs" | "lib.rs")
        );
    let module_directory = if owns_directory {
        directory.to_path_buf()
    } else {
        directory.join(file.file_stem().expect("a source file has a stem"))
    };
    let mut path_attribute: Option<String> = None;
    for line in source.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#[path = \"") {
            path_attribute = rest.strip_suffix("\"]").map(str::to_string);
            continue;
        }
        if line.starts_with("#[") {
            continue;
        }
        let declaration = line.strip_prefix("pub ").unwrap_or(line);
        if let Some(name) = declaration
            .strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
        {
            let next = match path_attribute.take() {
                Some(path) => directory.join(path),
                None => {
                    let flat = module_directory.join(format!("{name}.rs"));
                    if flat.exists() {
                        flat
                    } else {
                        module_directory.join(name).join("mod.rs")
                    }
                }
            };
            module_files(&next, false, out);
        }
        path_attribute = None;
    }
}

/// The lines of `source` that carry a libtest attribute.
fn libtest_attributes(source: &str) -> Vec<String> {
    let test = concat!("#[", "test]");
    let ignore = concat!("#[", "ignore");
    source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(test) || line.starts_with(ignore))
        .map(str::to_string)
        .collect()
}

/// §190.2 acceptance 3: no file of a two-phase binary carries a libtest
/// attribute, which the harness would never run, and every root denies
/// dead code, so an unlisted test function fails the build.
fn no_two_phase_binary_carries_a_libtest_attribute() {
    let roots = two_phase_roots();
    assert!(roots.len() >= 15, "two-phase roots: {roots:?}");
    let mut findings = Vec::new();
    let mut scanned = Vec::new();
    for root in &roots {
        let root_source = std::fs::read_to_string(root).expect("read the root");
        if !root_source.contains("#![deny(dead_code)]") {
            findings.push(format!("{}: no #![deny(dead_code)]", root.display()));
        }
        let mut files = Vec::new();
        module_files(root, true, &mut files);
        for file in files {
            let source = std::fs::read_to_string(&file).expect("read a module file");
            for line in libtest_attributes(&source) {
                findings.push(format!("{}: {line}", file.display()));
            }
            scanned.push(file);
        }
    }
    assert!(findings.is_empty(), "{}", findings.join("\n"));
    // The scan follows `mod` and `#[path]` declarations into the bodies.
    for body in [
        "cemit/operations.rs",
        "main_thread/../interop.rs",
        "native_fixture.rs",
    ] {
        assert!(
            scanned.iter().any(|file| file.ends_with(body)),
            "the scan missed {body}"
        );
    }
}

/// The control of the test above: the same scan finds both attributes.
fn the_libtest_attribute_scan_finds_both_attributes() {
    let source = format!(
        "{}\nfn a() {{}}\n    {}\nfn b() {{}}\n",
        concat!("#[", "test]"),
        concat!("#[", "ignore = \"slow\"]")
    );
    assert_eq!(libtest_attributes(&source).len(), 2);
}

fn main() -> ExitCode {
    main_thread::run(&main_thread_tests![
        parallel: [
            a_worker_run_completes_while_other_threads_start_threads,
            no_two_phase_binary_carries_a_libtest_attribute,
            the_libtest_attribute_scan_finds_both_attributes,
            #[cfg(unix)]
            a_phase_1_native_run_is_the_rule_3_error,
        ],
        main_thread: [
            #[cfg(unix)]
            a_native_run_with_a_second_thread_is_the_rule_3_error,
            a_native_run_on_the_only_thread_runs_the_program,
            a_native_run_right_after_joins_runs_the_program,
        ],
    ])
}
