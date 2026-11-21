//! The stress sweep of automatic collection (`compiler.md` §342.2 items 1,
//! 7, and 8).
//!
//! Each accept entry and each trap entry runs in the dev JIT, the ship tier,
//! and the interpreter with collection mode 2: each poll and each outermost
//! exit collects. Each run is compared with the golden of its entry by the
//! comparison of the standing sweeps: the stdout bytes for an accept entry
//! (`golden.rs`), and the trap kind, message, position, and stdout for a trap
//! entry (`cemit.rs`, with the expectations of `support/trap_corpus.rs`).
//! [`exclusion`] is the one table of runs that the sweep does not make.
//!
//! Each run is a child process of this binary with a time limit
//! ([`CHILD_LIMIT`]), so one slow entry cannot hold the sweep, and a run
//! that stops the process stops only its child. The dev JIT run with a
//! collection mode stays in the child's process (`compiler.md` §190.1).
//!
//! Cost: one child for each run, on the worker pool of the standing sweeps
//! (`support/pool.rs`). Measured on an arm64 macOS host with 594 entries
//! and 1,666 runs: 68 s in a debug build, 36 to 45 s in a release build.
//! The slowest run is one ship-tier entry at 12 s (debug). The sweep runs
//! in `tools/gate.sh full` only: it runs when
//! `SUBSCRIPT_FULL_INTERPRETER_SWEEP` is set, which the release step of the
//! full gate sets, and prints a `gate-skip:` line in each other run.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use subscript_codegen::interpreter::{interpret_with_collection, InterpretError};
use subscript_codegen::lir::lower_module;
use subscript_codegen::{run_c_aot_configured, run_jit_configured, NativeLibrary, RunConfig};
use subscript_codegen::{EntryArg, ReloadSession, RunError, TrapReport};
use subscript_compiler::SourceFile;
use subscript_runtime::context::CollectionMode;
use subscript_runtime::TrapKind;

#[path = "support/accept_runs.rs"]
mod accept_runs;
#[allow(dead_code)]
mod corpus;
// This target uses only part of the shared file-provider helpers.
#[allow(dead_code)]
#[path = "support/files.rs"]
mod file_fixture;
#[path = "support/native_fixture.rs"]
mod native_fixture;
#[path = "support/pool.rs"]
mod pool;
// This target uses only part of the shared trap helpers.
#[allow(dead_code)]
#[path = "support/trap_corpus.rs"]
mod trap_corpus;

/// The environment variable that runs one entry in one tier, as
/// `<accept|trap> <id> <tier>`, instead of the sweep.
const CHILD_ENV: &str = "SUBSCRIPT_COLLECTION_STRESS_CHILD";
/// The file where a child writes its outcome.
const RESULT_ENV: &str = "SUBSCRIPT_COLLECTION_STRESS_RESULT";
/// The environment variable that selects the sweep (`tools/gate.sh full`).
const FULL_SWEEP_ENV: &str = "SUBSCRIPT_FULL_INTERPRETER_SWEEP";
/// The test that a child runs.
const SWEEP_TEST: &str = "collection_stress_sweep";
/// The time limit of one child.
const CHILD_LIMIT: Duration = Duration::from_secs(300);
/// The ship-tier pre-init hook that sets mode 2.
const SETUP_HOOK: &str = "subscript_stress_collection_setup";
/// The ship-tier post-run hook that writes the outermost exit count
/// (`compiler.md` §345 rule 4).
const EXIT_REPORT_HOOK: &str = "subscript_stress_exit_report";
/// The file in the child scratch directory where [`EXIT_REPORT_HOOK`]
/// writes the count. The script stdout does not carry it.
const EXIT_COUNT_FILE: &str = "exit-count";
/// The ship-tier hook of the scratch file provider (`support/files.rs`).
const FILES_SETUP_HOOK: &str = "subscript_test_files_setup";
/// The file-provider line of a corpus entry.
const FILE_PROVIDER_LINE: &str = "// file-provider: scratch";
/// The accept entry that the sweep does not run for its cost.
const COST_EXCLUSION: &str = "a22-matrix-propagation";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    Accept,
    Trap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    Dev,
    Ship,
    Interpreter,
}

const TIERS: [Tier; 3] = [Tier::Dev, Tier::Ship, Tier::Interpreter];

impl Tier {
    fn name(self) -> &'static str {
        match self {
            Tier::Dev => "dev-JIT",
            Tier::Ship => "ship-C-AOT",
            Tier::Interpreter => "interpreter",
        }
    }

    fn from_name(name: &str) -> Tier {
        TIERS
            .into_iter()
            .find(|tier| tier.name() == name)
            .unwrap_or_else(|| panic!("unknown tier {name}"))
    }
}

/// One corpus entry.
#[derive(Clone, Debug)]
struct Entry {
    category: Category,
    id: String,
}

impl Entry {
    fn accept(id: &str) -> Entry {
        Entry {
            category: Category::Accept,
            id: id.to_string(),
        }
    }

    fn trap(id: &str) -> Entry {
        Entry {
            category: Category::Trap,
            id: id.to_string(),
        }
    }

    fn sources(&self) -> Vec<SourceFile> {
        match self.category {
            Category::Accept => corpus::entry_sources(&corpus::corpus_accept(), &self.id),
            Category::Trap => trap_corpus::trap_sources(&trap_corpus::corpus_trap(), &self.id),
        }
    }

    /// The header source: `main.ts` of a directory entry.
    fn header_path(&self) -> PathBuf {
        match self.category {
            Category::Accept => {
                let accept = corpus::corpus_accept();
                if accept.join(&self.id).is_dir() {
                    accept.join(&self.id).join("main.ts")
                } else {
                    accept.join(format!("{}.ts", self.id))
                }
            }
            Category::Trap => trap_corpus::corpus_trap().join(format!("{}.ts", self.id)),
        }
    }

    /// The native libraries of the entry, or `None` when the interop
    /// fixture is not available here.
    fn libraries(&self, sources: &[SourceFile]) -> Option<Vec<NativeLibrary>> {
        match self.category {
            Category::Accept => accept_runs::native_libraries(sources),
            Category::Trap => trap_corpus::trap_native_libraries(sources),
        }
    }
}

fn uses_file_provider(sources: &[SourceFile]) -> bool {
    sources
        .iter()
        .any(|file| file.source.lines().any(|line| line == FILE_PROVIDER_LINE))
}

/// True when a host drives the entry through its exports. The dev tier runs
/// such an entry in a reload session.
fn host_driven(entry: &Entry) -> bool {
    match entry.category {
        Category::Accept => accept_runs::host_hooks(&entry.id) != (None, None),
        Category::Trap => entry.id == "t50-wire-entry-unknown-value",
    }
}

/// The table of runs that the sweep does not make, with the reason of each
/// (`compiler.md` §342.2 item 1). `None` means that the sweep runs the
/// entry in the tier.
fn exclusion(entry: &Entry, sources: &[SourceFile], tier: Tier) -> Option<String> {
    if entry.id == COST_EXCLUSION {
        return Some(
            "cost: 10,000 live nodes and a full collection at each backedge; \
             the release dev JIT run did not end in 180 s"
                .to_string(),
        );
    }
    if entry.category == Category::Trap {
        if let Some(reason) = trap_corpus::sweep_exclusion(&entry.id) {
            return Some(reason.to_string());
        }
    }
    if entry.libraries(sources).is_none() {
        return Some("the interop fixture is not available here (compiler.md §11c)".to_string());
    }
    match tier {
        Tier::Ship
            if entry.category == Category::Trap
                && trap_corpus::freed_handle_diagnostic(&entry.id) =>
        {
            Some("Q6/§8.1b: the ship-tier result is undefined".to_string())
        }
        Tier::Interpreter => interpreter_exclusion(entry),
        _ => None,
    }
}

/// The interpreter rule of the standing sweeps: the `interpreter:` header
/// field. A trap entry that injects an allocation failure also needs the
/// injection, and a host-driven entry needs a host that calls its exports;
/// the interpreter has neither. A trap entry whose run reports
/// [`Outcome::Unsupported`] is also not run; the sweep prints the reason
/// that the interpreter gives.
fn interpreter_exclusion(entry: &Entry) -> Option<String> {
    if host_driven(entry) {
        return Some("the interpreter has no host that calls the exports".to_string());
    }
    let path = entry.header_path();
    let source = std::fs::read_to_string(&path).expect("read corpus header");
    let header = subscript_compiler::language_reference::parse_header(&path, &source)
        .unwrap_or_else(|error| panic!("{error}"));
    if let Some(reason) = header.interpreter_exclusion {
        return Some(format!("interpreter: no — {reason}"));
    }
    if entry.category == Category::Trap
        && trap_corpus::allocation_failure_count(&entry.id).is_some()
    {
        return Some("the interpreter has no allocation-failure injection".to_string());
    }
    None
}

/// The result of one run.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    /// The run ended with no trap. `exits` is the outermost exit count
    /// (`compiler.md` §342 rule 16), and `collections` the automatic
    /// collection count (rule 7). The ship tier gives the exit count
    /// through [`EXIT_COUNT_FILE`] (§345 rule 4), and no collection count.
    Complete {
        stdout: Vec<u8>,
        exits: Option<u64>,
        collections: Option<u64>,
    },
    /// The run trapped.
    Trap {
        kind: String,
        message: String,
        file: String,
        line: u32,
        col: u32,
        stdout: Vec<u8>,
    },
    /// The run failed outside the trap protocol.
    Failed(String),
    /// The interpreter cannot link a dependency of the program, such as a
    /// native library or the worker adapter.
    Unsupported(String),
}

impl Outcome {
    fn from_report(report: TrapReport) -> Outcome {
        Outcome::Trap {
            kind: report.rule.rule().to_string(),
            message: report.message,
            file: report.pos.file,
            line: report.pos.line,
            col: report.pos.col,
            stdout: report.stdout,
        }
    }

    fn render(&self) -> String {
        match self {
            Outcome::Complete { stdout, exits, .. } => format!(
                "Complete(stdout={:?}, exits={exits:?})",
                String::from_utf8_lossy(stdout)
            ),
            Outcome::Trap {
                kind,
                message,
                file,
                line,
                col,
                stdout,
            } => format!(
                "Trap(kind={kind}, message={message:?}, position={file}:{line}:{col}, stdout={:?})",
                String::from_utf8_lossy(stdout)
            ),
            Outcome::Failed(text) => format!("Failed({text})"),
            Outcome::Unsupported(reason) => format!("Unsupported({reason})"),
        }
    }

    fn encode(&self) -> String {
        match self {
            Outcome::Complete {
                stdout,
                exits,
                collections,
            } => {
                let count = |value: &Option<u64>| value.map_or("-".to_string(), |n| n.to_string());
                format!(
                    "complete\n{}\n{}\n{}\n",
                    hex(stdout),
                    count(exits),
                    count(collections)
                )
            }
            Outcome::Trap {
                kind,
                message,
                file,
                line,
                col,
                stdout,
            } => format!(
                "trap\n{kind}\n{}\n{}\n{line}\n{col}\n{}\n",
                hex(message.as_bytes()),
                hex(file.as_bytes()),
                hex(stdout)
            ),
            Outcome::Failed(text) => format!("failed\n{}\n", hex(text.as_bytes())),
            Outcome::Unsupported(reason) => {
                format!("unsupported\n{}\n", hex(reason.as_bytes()))
            }
        }
    }

    fn decode(text: &str) -> Outcome {
        let fields: Vec<&str> = text.lines().collect();
        let string = |index: usize| String::from_utf8(unhex(fields[index])).expect("utf-8 field");
        match fields.first().copied() {
            Some("complete") if fields.len() == 4 => Outcome::Complete {
                stdout: unhex(fields[1]),
                exits: fields[2].parse().ok(),
                collections: fields[3].parse().ok(),
            },
            Some("trap") if fields.len() == 7 => Outcome::Trap {
                kind: fields[1].to_string(),
                message: string(2),
                file: string(3),
                line: fields[4].parse().expect("line"),
                col: fields[5].parse().expect("column"),
                stdout: unhex(fields[6]),
            },
            Some("failed") if fields.len() == 2 => Outcome::Failed(string(1)),
            Some("unsupported") if fields.len() == 2 => Outcome::Unsupported(string(1)),
            _ => Outcome::Failed(format!("the child wrote a malformed outcome: {text:?}")),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex byte"))
        .collect()
}

/// The source of the ship-tier hook library that sets mode 2 before the
/// initializer. An entry with the scratch file provider also installs the
/// provider in the same hook, because the runner takes one pre-init hook.
/// The library also defines [`EXIT_REPORT_HOOK`]: it calls `post_run`, the
/// post-run hook of the entry if it has one, then writes the outermost exit
/// count to [`EXIT_COUNT_FILE`] in `scratch` (`compiler.md` §345 rule 4).
fn setup_hook_library(
    scratch: &Path,
    file_provider: bool,
    post_run: Option<&str>,
) -> NativeLibrary {
    let source = scratch.join("stress_setup.c");
    let (mut declaration, call) = if file_provider {
        (
            format!("void {FILES_SETUP_HOOK}(subscript_rt_context* ctx);\n"),
            format!("  {FILES_SETUP_HOOK}(ctx);\n"),
        )
    } else {
        (String::new(), String::new())
    };
    let post_call = match post_run {
        Some(hook) => {
            declaration.push_str(&format!("void {hook}(subscript_rt_context* ctx);\n"));
            format!("  {hook}(ctx);\n")
        }
        None => String::new(),
    };
    let count_path = c_string(&scratch.join(EXIT_COUNT_FILE));
    std::fs::write(
        &source,
        format!(
            "#include <stdint.h>\n\
             #include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             uint64_t subscript_rt_outermost_exit_count(const subscript_rt_context* ctx);\n\
             {declaration}\
             void {SETUP_HOOK}(subscript_rt_context* ctx) {{\n\
             {call}\
               if (subscript_rt_ctx_set_collection(ctx, 2, 0u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             void {EXIT_REPORT_HOOK}(subscript_rt_context* ctx) {{\n\
             {post_call}\
               FILE* out = fopen({count_path}, \"w\");\n\
               if (out != NULL) {{\n\
                 fprintf(out, \"%llu\\n\", (unsigned long long)subscript_rt_outermost_exit_count(ctx));\n\
                 fclose(out);\n\
               }}\n\
             }}\n"
        ),
    )
    .expect("write the setup hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only host hooks; no script calls it.
    unsafe { NativeLibrary::new(vec![repo.join("runtime/include")], vec![source], Vec::new()) }
}

/// `path` as a C string literal.
fn c_string(path: &Path) -> String {
    let text = path.to_str().expect("a UTF-8 scratch path");
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The count that [`EXIT_REPORT_HOOK`] wrote to `scratch`, if any.
fn reported_exit_count(scratch: &Path) -> Option<u64> {
    std::fs::read_to_string(scratch.join(EXIT_COUNT_FILE))
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

fn run_result(result: Result<subscript_codegen::RunOutput, RunError>) -> Outcome {
    match result {
        Ok(output) => Outcome::Complete {
            stdout: output.stdout,
            exits: output
                .memory_accounting
                .as_ref()
                .map(|accounting| accounting.outermost_exits),
            collections: output
                .memory_accounting
                .map(|accounting| accounting.automatic_collections),
        },
        Err(RunError::Trap(report)) => Outcome::from_report(report),
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

/// The dev JIT run of one entry with mode 2. The run reads the accounting,
/// so it stays in this process. A host-driven entry runs in a reload
/// session ([`dev_host_run`]).
fn dev_run(entry: &Entry, sources: &[SourceFile]) -> Outcome {
    let libraries = entry
        .libraries(sources)
        .expect("the table excludes the run");
    if host_driven(entry) {
        return dev_host_run(entry, sources, &libraries);
    }
    let mut fixture = None;
    let mut config = if uses_file_provider(sources) {
        fixture.insert(file_fixture::Fixture::new()).config()
    } else {
        RunConfig::default().with_native_libraries(&libraries)
    };
    config.collection = Some((CollectionMode::EachPoll, 0));
    config.memory_accounting = true;
    if entry.category == Category::Trap {
        config.fail_alloc_after = trap_corpus::allocation_failure_count(&entry.id);
        config.freed_handle_diagnostics = trap_corpus::freed_handle_diagnostic(&entry.id)
            || trap_corpus::callback_userdata_diagnostic(&entry.id);
    }
    let outcome = run_result(run_jit_configured(sources, config));
    drop(fixture);
    outcome
}

/// The dev JIT run of a host-driven entry with mode 2. A reload session
/// sets the mode before the initializer and calls the exports that the
/// ship-tier hooks of the entry call (`support/accept_runs.rs`,
/// `corpus/interop/wire-enum.c`). The ship-tier hooks call them with no
/// bracket. The session brackets each call, so each call of the session is
/// an outermost exit (`compiler.md` §342 rule 8a). The session gives no
/// exit count and no collection count.
fn dev_host_run(entry: &Entry, sources: &[SourceFile], libraries: &[NativeLibrary]) -> Outcome {
    let mut config = RunConfig::default().with_native_libraries(libraries);
    config.collection = Some((CollectionMode::EachPoll, 0));
    let mut session = match ReloadSession::new_configured(sources, config) {
        Ok(session) => session,
        Err(error) => return run_result(Err(error)),
    };
    let fixture = native_fixture::fixture();
    let host_state = |entry_id: &str| {
        entry_id == accept_runs::HOST_OWNED_STATE_ID
            || entry_id == accept_runs::HANDLE_ENTRY_PARAM_ID
    };
    let fixture = || fixture.as_ref().expect("the table excludes the run");
    if host_state(&entry.id) {
        fixture().host_owned_state_pre_entry();
    }
    let parameters = if entry.id == accept_runs::HANDLE_ENTRY_PARAM_ID {
        let state = fixture().host_owned_state_borrow_and_advance();
        session.call_export_with("adopt", &[EntryArg::Handle(state), EntryArg::I32(7)])
    } else if entry.id == accept_runs::WIRE_ENTRY_PARAM_ID {
        session.call_export_with("configure", &[EntryArg::I32(23), EntryArg::I32(5)])
    } else if entry.category == Category::Trap {
        session.call_export_with("configure", &[EntryArg::I32(12345), EntryArg::I32(5)])
    } else {
        Ok(())
    };
    let run = parameters.and_then(|()| session.run_main());
    if host_state(&entry.id) {
        fixture().host_owned_state_post_run();
    }
    match run {
        Ok(()) => Outcome::Complete {
            stdout: session.take_output(),
            exits: None,
            collections: None,
        },
        Err(error) => run_result(Err(error)),
    }
}

/// The ship-tier run of one entry with mode 2, set by the pre-init hook.
fn ship_run(entry: &Entry, sources: &[SourceFile], scratch: &Path) -> Outcome {
    let file_provider = uses_file_provider(sources);
    let fixture = file_provider.then(file_fixture::Fixture::new);
    let mut libraries = if file_provider {
        Vec::new()
    } else {
        entry
            .libraries(sources)
            .expect("the table excludes the run")
    };
    if let Some(fixture) = &fixture {
        libraries.push(fixture.library());
    }
    let (pre_entry, post_run) = match entry.category {
        Category::Accept => accept_runs::host_hooks(&entry.id),
        Category::Trap if host_driven(entry) => (Some("subWireEntryDriveUnknown"), None),
        Category::Trap => (None, None),
    };
    libraries.push(setup_hook_library(scratch, file_provider, post_run));
    let mut config = RunConfig::default().with_native_libraries(&libraries);
    if file_provider {
        config = config.with_enabled_modules(&["node:fs/promises"]);
    }
    config.pre_init_hook = Some(SETUP_HOOK);
    config.pre_entry_hook = pre_entry;
    config.post_run_hook = Some(EXIT_REPORT_HOOK);
    if entry.category == Category::Trap {
        config.fail_alloc_after = trap_corpus::allocation_failure_count(&entry.id);
        config.freed_handle_diagnostics = trap_corpus::callback_userdata_diagnostic(&entry.id);
    }
    let _ = std::fs::remove_file(scratch.join(EXIT_COUNT_FILE));
    match run_result(run_c_aot_configured(sources, config)) {
        Outcome::Complete { stdout, .. } => Outcome::Complete {
            stdout,
            exits: reported_exit_count(scratch),
            collections: None,
        },
        other => other,
    }
}

/// The interpreter run of one entry with mode 2. An `Internal` trap is the
/// failure of the allocation-fact check (`compiler.md` §342 rule 10a).
fn interpreter_run(sources: &[SourceFile]) -> Outcome {
    let hir = match corpus::check_program(sources) {
        Ok(hir) => hir,
        Err(diagnostics) => return Outcome::Failed(format!("checker rejected: {diagnostics:?}")),
    };
    let module = match lower_module(&hir) {
        Ok(module) => module,
        Err(error) => return Outcome::Failed(format!("lower failed: {error}")),
    };
    match interpret_with_collection(&module, CollectionMode::EachPoll, 0) {
        Ok((stdout, run)) => Outcome::Complete {
            stdout,
            exits: Some(run.outermost_exits),
            collections: Some(run.automatic_collections),
        },
        Err(error) => {
            let stdout = error.output().to_vec();
            let source = match error {
                InterpretError::Execution { source, .. } => *source,
                other => other,
            };
            match source {
                InterpretError::Trap {
                    kind,
                    runtime_kind,
                    pos,
                    message,
                } => Outcome::Trap {
                    kind: runtime_kind.map_or(kind, |kind| kind.rule().to_string()),
                    message,
                    file: pos.file,
                    line: pos.line,
                    col: pos.col,
                    stdout,
                },
                InterpretError::Unsupported { reason } => Outcome::Unsupported(reason),
                other => Outcome::Failed(format!(
                    "interpreter error: {other}; stdout {:?}",
                    String::from_utf8_lossy(&stdout)
                )),
            }
        }
    }
}

/// The child side: runs the entry and tier that [`CHILD_ENV`] names and
/// writes the outcome to the file that [`RESULT_ENV`] names.
fn child_main(spec: &str) {
    let mut parts = spec.split(' ');
    let (Some(category), Some(id), Some(tier), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        panic!("malformed child spec {spec:?}");
    };
    let entry = match category {
        "accept" => Entry::accept(id),
        "trap" => Entry::trap(id),
        other => panic!("unknown category {other}"),
    };
    let result = PathBuf::from(std::env::var_os(RESULT_ENV).expect("the result path"));
    let scratch = result.parent().expect("a scratch directory").to_path_buf();
    let sources = entry.sources();
    let outcome = match Tier::from_name(tier) {
        Tier::Dev => dev_run(&entry, &sources),
        Tier::Ship => ship_run(&entry, &sources, &scratch),
        Tier::Interpreter => interpreter_run(&sources),
    };
    std::fs::write(&result, outcome.encode()).expect("write the outcome");
}

/// Stops the child and each process that it started.
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The parent side: runs one entry in one tier in a child of this binary
/// and returns its outcome and wall time.
fn run_in_child(entry: &Entry, tier: Tier, slot: usize) -> (Outcome, Duration) {
    let started = Instant::now();
    let scratch = std::env::temp_dir().join(format!(
        "subscript-collection-stress-{}-{slot}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create the child scratch directory");
    let result = scratch.join("outcome");
    let category = match entry.category {
        Category::Accept => "accept",
        Category::Trap => "trap",
    };
    let mut command = Command::new(std::env::current_exe().expect("the test binary"));
    command
        .args([SWEEP_TEST, "--exact", "--test-threads", "1"])
        .env(
            CHILD_ENV,
            format!("{category} {} {}", entry.id, tier.name()),
        )
        .env(RESULT_ENV, &result)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(scratch.join("stderr")).expect("create the child stderr"));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().expect("start the child");
    let status = loop {
        match child.try_wait().expect("wait for the child") {
            Some(status) => break Some(status),
            None if started.elapsed() > CHILD_LIMIT => {
                stop(&mut child);
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let outcome = match (status, std::fs::read_to_string(&result)) {
        (None, _) => Outcome::Failed(format!("the run did not end in {CHILD_LIMIT:?}")),
        (Some(_), Ok(text)) => Outcome::decode(&text),
        (Some(status), Err(_)) => {
            let stderr = std::fs::read(scratch.join("stderr")).unwrap_or_default();
            let stderr = String::from_utf8_lossy(&stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(12).collect();
            Outcome::Failed(format!(
                "the child ended with {status} and wrote no outcome: {}",
                tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
            ))
        }
    };
    let _ = std::fs::remove_dir_all(&scratch);
    (outcome, started.elapsed())
}

/// The golden of one entry.
#[derive(Clone, Debug)]
enum Golden {
    Accept(Vec<u8>),
    Trap {
        kind: &'static str,
        file: String,
        line: u32,
        col: u32,
        message: Option<&'static str>,
        stdout: Vec<u8>,
    },
}

fn golden(entry: &Entry) -> Golden {
    match entry.category {
        Category::Accept => {
            Golden::Accept(corpus::golden_bytes(&corpus::corpus_accept(), &entry.id))
        }
        Category::Trap => {
            let (kind, line, col) = trap_corpus::trap_expectation(&entry.id);
            // §112 rule 1: line 0 and column 0 carry the empty file name.
            let file = if line == 0 && col == 0 {
                String::new()
            } else {
                format!("{}.ts", entry.id)
            };
            Golden::Trap {
                kind: kind.rule(),
                file,
                line,
                col,
                message: trap_corpus::regex_error_message(&entry.id)
                    .or_else(|| trap_corpus::trap_message(&entry.id)),
                stdout: trap_corpus::trap_expected(&trap_corpus::corpus_trap(), &entry.id),
            }
        }
    }
}

/// Compares one outcome with the golden. Returns the difference, if any.
/// The interpreter comparison of a trap omits the message, as the
/// interpreter trap test of `lir.rs` does.
fn compare(id: &str, tier: Tier, golden: &Golden, outcome: &Outcome) -> Option<String> {
    let equal = match (golden, outcome) {
        (Golden::Accept(expected), Outcome::Complete { stdout, .. }) => stdout == expected,
        (
            Golden::Trap {
                kind,
                file,
                line,
                col,
                message,
                stdout,
            },
            Outcome::Trap {
                kind: actual_kind,
                message: actual_message,
                file: actual_file,
                line: actual_line,
                col: actual_col,
                stdout: actual_stdout,
            },
        ) => {
            actual_kind == kind
                && actual_file == file
                && actual_line == line
                && actual_col == col
                && (tier == Tier::Interpreter
                    || message.is_none_or(|message| actual_message == message))
                && actual_stdout == stdout
        }
        _ => false,
    };
    (!equal).then(|| {
        format!(
            "{id}: {} differs from the golden\n  expected {}\n  actual   {}",
            tier.name(),
            render_golden(golden),
            outcome.render()
        )
    })
}

fn render_golden(golden: &Golden) -> String {
    match golden {
        Golden::Accept(stdout) => format!("Complete(stdout={:?})", String::from_utf8_lossy(stdout)),
        Golden::Trap {
            kind,
            file,
            line,
            col,
            message,
            stdout,
        } => format!(
            "Trap(kind={kind}, message={message:?}, position={file}:{line}:{col}, stdout={:?})",
            String::from_utf8_lossy(stdout)
        ),
    }
}

/// True for the interpreter error of the allocation-fact check
/// (`compiler.md` §342 rule 10a).
fn is_internal(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Trap { kind, .. } if kind == TrapKind::Internal.rule())
}

/// Acceptance 1, 7, and 8 of `compiler.md` §342.2. The module comment states
/// the cost and the gate shape.
#[test]
fn collection_stress_sweep() {
    if let Ok(spec) = std::env::var(CHILD_ENV) {
        child_main(&spec);
        return;
    }
    if std::env::var_os(FULL_SWEEP_ENV).is_none() {
        use std::io::Write;
        writeln!(
            std::io::stdout().lock(),
            "\ngate-skip: collection_stress_sweep runs in tools/gate.sh full only; set {FULL_SWEEP_ENV}=1"
        )
        .expect("write the gate skip line");
        return;
    }
    let started = Instant::now();
    let accept = corpus::corpus_accept();
    let mut entries: Vec<Entry> = corpus::golden_ids(&accept)
        .iter()
        .map(|id| Entry::accept(id))
        .collect();
    entries.extend(
        trap_corpus::trap_ids(&trap_corpus::corpus_trap())
            .iter()
            .map(|id| Entry::trap(id)),
    );

    let mut runs: Vec<(usize, Tier)> = Vec::new();
    let mut not_run: Vec<(String, Tier, String)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let sources = entry.sources();
        for tier in TIERS {
            match exclusion(entry, &sources, tier) {
                Some(reason) => not_run.push((entry.id.clone(), tier, reason)),
                None => runs.push((index, tier)),
            }
        }
    }
    let slots: Vec<(usize, (usize, Tier))> = runs.iter().copied().enumerate().collect();
    let outcomes = pool::map_in_order(&slots, |(slot, (index, tier))| {
        run_in_child(&entries[*index], *tier, *slot)
    });

    let mut differences = Vec::new();
    let mut internal = Vec::new();
    let mut per_tier = [(0usize, 0usize); 3];
    let mut exits: std::collections::HashMap<usize, [Option<u64>; 3]> =
        std::collections::HashMap::new();
    let mut slowest = (Duration::ZERO, String::new());
    let mut per_tier_collections = [0u64; 3];
    let mut ship_without_count = Vec::new();
    for ((index, tier), (outcome, elapsed)) in runs.iter().zip(&outcomes) {
        let entry = &entries[*index];
        let column = TIERS.iter().position(|t| t == tier).expect("a tier");
        if let (Category::Trap, Outcome::Unsupported(reason)) = (entry.category, outcome) {
            not_run.push((
                entry.id.clone(),
                *tier,
                format!("the interpreter reports: {reason}"),
            ));
            continue;
        }
        per_tier[column].0 += 1;
        if *elapsed > slowest.0 {
            slowest = (*elapsed, format!("{} {}", entry.id, tier.name()));
        }
        if *tier == Tier::Interpreter && is_internal(outcome) {
            internal.push(format!("{}: {}", entry.id, outcome.render()));
        }
        match compare(&entry.id, *tier, &golden(entry), outcome) {
            Some(difference) => differences.push(difference),
            None => per_tier[column].1 += 1,
        }
        if let Outcome::Complete {
            exits: count,
            collections,
            ..
        } = outcome
        {
            // §345 rule 4: the exit count is recorded apart from the
            // collection count, which the ship tier does not give.
            exits.entry(*index).or_default()[column] = *count;
            per_tier_collections[column] += collections.unwrap_or(0);
            if *tier == Tier::Ship && count.is_none() {
                ship_without_count.push(entry.id.clone());
            }
        }
    }

    // §345 rule 4: the three tiers mark the same count of outermost exits
    // in each entry that each of them runs to the end. Where one tier gives
    // no count, the two counts that remain are compared. A trapped run, a
    // run that the table excludes, and a host-driven dev JIT run
    // ([`dev_host_run`]) give no count.
    let mut exit_compared = 0usize;
    let mut exit_compared_three = 0usize;
    let mut exit_differences = Vec::new();
    let mut keys: Vec<&usize> = exits.keys().collect();
    keys.sort();
    for index in keys {
        let id = &entries[*index].id;
        let mut compare_pair = |first: (&str, u64), second: (&str, u64)| {
            if first.1 != second.1 {
                exit_differences.push(format!(
                    "{id}: outermost exits {} {} != {} {}",
                    first.0, first.1, second.0, second.1
                ));
            }
        };
        match exits[index] {
            [Some(dev), Some(ship), Some(interpreter)] => {
                exit_compared += 1;
                exit_compared_three += 1;
                compare_pair(("dev-JIT", dev), ("ship-C-AOT", ship));
                compare_pair(("dev-JIT", dev), ("interpreter", interpreter));
            }
            [Some(dev), Some(ship), None] => {
                exit_compared += 1;
                compare_pair(("dev-JIT", dev), ("ship-C-AOT", ship));
            }
            [Some(dev), None, Some(interpreter)] => {
                exit_compared += 1;
                compare_pair(("dev-JIT", dev), ("interpreter", interpreter));
            }
            [None, Some(ship), Some(interpreter)] => {
                exit_compared += 1;
                compare_pair(("ship-C-AOT", ship), ("interpreter", interpreter));
            }
            // `compiler.md` §345 rule 4 compares the counts of the runs to the
            // end with no trap. With one count, no second count exists to compare. A
            // ship run to the end with no count is a failure of its own
            // (`ship_without_count`).
            [Some(_), None, None] | [None, Some(_), None] | [None, None, Some(_)] => {}
            // `compiler.md` §345 rule 4: no tier gave a count, so nothing is
            // compared. A
            // host-driven dev JIT run gives none, and the other two tiers
            // trapped or the table excludes them.
            [None, None, None] => {}
        }
    }

    for (id, tier, reason) in &not_run {
        println!("not run: {id} {}: {reason}", tier.name());
    }
    for (column, tier) in TIERS.iter().enumerate() {
        let skipped = not_run.iter().filter(|(_, t, _)| t == tier).count();
        eprintln!(
            "collection stress {}: {} run, {} equal, {skipped} not run, {} automatic collections",
            tier.name(),
            per_tier[column].0,
            per_tier[column].1,
            per_tier_collections[column]
        );
    }
    eprintln!(
        "collection stress: {} entries, {exit_compared} exit counts compared, {exit_compared_three} of them in three tiers, slowest run {} in {:.1} s, {:.1} s",
        entries.len(),
        slowest.1,
        slowest.0.as_secs_f64(),
        started.elapsed().as_secs_f64()
    );
    let mut failures = differences;
    // The control of the mode: a run with mode 2 collects at its polls and
    // exits. The sweep does not read the collection count of the ship tier,
    // so its control reads live bytes after an unbracketed call.
    for (column, tier) in TIERS.iter().enumerate() {
        if *tier != Tier::Ship && per_tier_collections[column] == 0 {
            failures.push(format!("{}: no automatic collection ran", tier.name()));
        }
    }
    failures.extend(ship_mode_control());
    failures.extend(ship_exit_count_control());
    failures.extend(
        ship_without_count
            .iter()
            .map(|id| format!("{id}: ship-C-AOT ran to the end and wrote no exit count")),
    );
    failures.extend(
        internal
            .iter()
            .map(|line| format!("rule 10a Internal error: {line}")),
    );
    failures.extend(exit_differences);
    assert!(
        failures.is_empty(),
        "{} collection stress failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// An export that allocates 1,000 objects in a loop with a backedge poll.
const CHURN: &str = r#"
class Cell { v: i32; constructor(v: i32) { this.v = v; } }
export function churn(): void {
  let s: i32 = 0;
  for (let i: i32 = 0; i < 1000; i += 1) { const c: Cell = new Cell(i); s += c.v; }
  print(`${s}`);
}
export function main(): void {}
"#;

/// Runs [`CHURN`] in the ship tier. A pre-entry hook calls `churn` with no
/// `subscript_rt_ctx_enter_script` and `subscript_rt_ctx_exit_script`
/// bracket, then prints live bytes, so no outermost exit collects before
/// the print: only a poll can. With `stress_setup`, the run has the
/// pre-init hook of the sweep ([`setup_hook_library`]). Returns live bytes.
fn ship_churn_live_bytes(scratch: &Path, stress_setup: bool) -> u64 {
    let hook = scratch.join("stress_churn.c");
    std::fs::write(
        &hook,
        "#include <stdio.h>\n\
         #include \"program.h\"\n\
         void subscript_stress_churn(subscript_rt_context* ctx) {\n\
           subscript_export_churn(ctx);\n\
           printf(\"live-bytes=%llu\\n\", (unsigned long long)subscript_rt_ctx_live_bytes(ctx));\n\
           fflush(stdout);\n\
         }\n",
    )
    .expect("write the churn hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only the pre-entry hook; no script calls it.
    let mut libraries = vec![unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hook], Vec::new())
    }];
    let mut config = RunConfig::default();
    if stress_setup {
        libraries.push(setup_hook_library(scratch, false, None));
        config.pre_init_hook = Some(SETUP_HOOK);
    }
    config = config.with_native_libraries(&libraries);
    config.pre_entry_hook = Some("subscript_stress_churn");
    let files = [SourceFile::new("churn.ts", CHURN)];
    let output = run_c_aot_configured(&files, config).expect("ship churn run");
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("499500"), "{text:?}");
    lines
        .next()
        .and_then(|line| line.strip_prefix("live-bytes="))
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("the hook prints live bytes: {text:?}"))
}

/// The control that the pre-init hook of the sweep sets mode 2 in the ship
/// tier, where no host function reads the collection count. With the hook,
/// the polls of `churn` collect, and live bytes after the call are under a
/// tenth of the run with the default mode 1, whose 1,000 objects are under
/// its 16 MiB headroom. Measured: 16 live bytes with the hook, and 16,016
/// with the default mode. The sweep runs it once. Cost: two ship-tier C
/// builds, about 1 s.
fn ship_mode_control() -> Option<String> {
    let scratch = std::env::temp_dir().join(format!(
        "subscript-collection-stress-control-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create the control directory");
    let stressed = ship_churn_live_bytes(&scratch, true);
    let default = ship_churn_live_bytes(&scratch, false);
    let _ = std::fs::remove_dir_all(&scratch);
    (stressed * 10 >= default).then(|| {
        format!(
            "ship-C-AOT: mode 2 did not take effect: live bytes {stressed}, default mode {default}"
        )
    })
}

/// Runs [`CHURN`] in the ship tier with the hooks of the sweep. A pre-entry
/// hook calls `churn`, with a `subscript_rt_ctx_enter_script` and
/// `subscript_rt_ctx_exit_script` bracket if `bracketed`. Returns the exit
/// count that [`EXIT_REPORT_HOOK`] wrote.
fn ship_churn_exit_count(scratch: &Path, bracketed: bool) -> Option<u64> {
    let hook = scratch.join("stress_exit_churn.c");
    let (enter, exit) = if bracketed {
        (
            "  subscript_rt_ctx_enter_script(ctx);\n",
            "  subscript_rt_ctx_exit_script(ctx);\n",
        )
    } else {
        ("", "")
    };
    std::fs::write(
        &hook,
        format!(
            "#include \"program.h\"\n\
             void subscript_stress_exit_churn(subscript_rt_context* ctx) {{\n\
             {enter}\
               subscript_export_churn(ctx);\n\
             {exit}\
             }}\n"
        ),
    )
    .expect("write the exit churn hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only the pre-entry hook; no script calls it.
    let libraries = vec![
        unsafe { NativeLibrary::new(vec![repo.join("runtime/include")], vec![hook], Vec::new()) },
        setup_hook_library(scratch, false, None),
    ];
    let mut config = RunConfig::default().with_native_libraries(&libraries);
    config.pre_init_hook = Some(SETUP_HOOK);
    config.pre_entry_hook = Some("subscript_stress_exit_churn");
    config.post_run_hook = Some(EXIT_REPORT_HOOK);
    let _ = std::fs::remove_file(scratch.join(EXIT_COUNT_FILE));
    let files = [SourceFile::new("churn.ts", CHURN)];
    let output = run_c_aot_configured(&files, config).expect("ship churn run");
    assert_eq!(output.stdout, b"499500\n", "the churn output");
    reported_exit_count(scratch)
}

/// The firing control of the ship-tier exit count (`compiler.md` §345
/// acceptance 4): an export call with no bracket gives a lower count than
/// the same call with the bracket. The sweep runs it once. Cost: two
/// ship-tier C builds, about 1 s.
fn ship_exit_count_control() -> Option<String> {
    let scratch = std::env::temp_dir().join(format!(
        "subscript-collection-stress-exit-control-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create the control directory");
    let unbracketed = ship_churn_exit_count(&scratch, false);
    let bracketed = ship_churn_exit_count(&scratch, true);
    let _ = std::fs::remove_dir_all(&scratch);
    match (unbracketed, bracketed) {
        (Some(unbracketed), Some(bracketed)) if unbracketed < bracketed => None,
        _ => Some(format!(
            "ship-C-AOT: the exit count did not see the bracket: unbracketed {unbracketed:?}, bracketed {bracketed:?}"
        )),
    }
}

/// The firing control of the sweep (core principle 9). One accept entry in
/// the dev JIT and one trap entry in the interpreter run through the child
/// path of the sweep. Each outcome equals its golden, and the same comparison
/// reports a difference for a golden with one changed byte or one changed
/// column. Cost: two children, about one second.
#[test]
fn the_sweep_comparison_reports_a_changed_golden() {
    let entry = Entry::accept("a01-hello");
    let expected = golden(&entry);
    let (outcome, _) = run_in_child(&entry, Tier::Dev, usize::MAX);
    assert_eq!(compare(&entry.id, Tier::Dev, &expected, &outcome), None);
    assert!(
        matches!(outcome, Outcome::Complete { exits: Some(3), .. }),
        "{outcome:?}"
    );
    let Golden::Accept(mut changed) = expected else {
        panic!("an accept golden");
    };
    changed[0] ^= 1;
    assert!(compare(&entry.id, Tier::Dev, &Golden::Accept(changed), &outcome).is_some());

    let entry = Entry::trap("t08-div-zero-expression");
    let expected = golden(&entry);
    let (outcome, _) = run_in_child(&entry, Tier::Interpreter, usize::MAX - 1);
    assert_eq!(
        compare(&entry.id, Tier::Interpreter, &expected, &outcome),
        None
    );
    let Golden::Trap {
        kind,
        file,
        line,
        col,
        message,
        stdout,
    } = expected
    else {
        panic!("a trap golden");
    };
    let changed = Golden::Trap {
        kind,
        file,
        line,
        col: col + 1,
        message,
        stdout,
    };
    assert!(compare(&entry.id, Tier::Interpreter, &changed, &outcome).is_some());
}
