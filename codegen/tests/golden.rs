//! The standing differential gate (`specs/blocks/compiler.md` §2, §11).
//!
//! For every run-set entry with a committed golden:
//! **dev-JIT bytes ≡ ship-C-AOT bytes ≡ golden bytes**, byte-exact, with
//! no normalization. The ship tier is C emission compiled by the
//! platform C compiler and linked with the runtime static library
//! (§11). The two tiers are separate lowerings, so this test
//! establishes their agreement by verification. The entry set is derived
//! from `corpus/accept/`, so a new entry or golden is picked up with no
//! edit here.
//!
//! There are no skips of the ship tier. A missing host C compiler, a
//! missing runtime static library, or a failing compile/link fails this
//! test: the gate machine is the development machine (§8.3).

// compiler.md §190.1 rule 3: a test function that the phase list does
// not name fails the build.
#![deny(dead_code)]

#[path = "support/main_thread.rs"]
mod main_thread;

mod corpus;
#[path = "support/native_fixture.rs"]
mod native_fixture;
#[path = "support/pool.rs"]
mod pool;

#[allow(dead_code)]
#[path = "support/files.rs"]
mod file_fixture;

use subscript_codegen::{
    run_c_aot_with_native_libraries_and_host_hooks, run_jit_with_native_libraries, NativeLibrary,
    RunError,
};
use subscript_codegen::{EntryArg, ReloadSession};

const HOST_OWNED_STATE_ID: &str = "a128-host-owned-state";
const HOST_OWNED_STATE_PRE_ENTRY: &str = "subHostOwnedStatePreEntry";
const HOST_OWNED_STATE_POST_RUN: &str = "subHostOwnedStatePostRun";
const HANDLE_ENTRY_PARAM_ID: &str = "a137-handle-entry-param";
const HANDLE_ENTRY_PARAM_PRE_ENTRY: &str = "subHostOwnedStateAdoptDrive";
const WIRE_ENTRY_PARAM_ID: &str = "a140-wire-entry-param";
const WIRE_ENTRY_PARAM_PRE_ENTRY: &str = "subWireEntryDrive";

fn host_hooks(id: &str) -> (Option<&'static str>, Option<&'static str>) {
    if id == HOST_OWNED_STATE_ID {
        (
            Some(HOST_OWNED_STATE_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
    } else if id == HANDLE_ENTRY_PARAM_ID {
        (
            Some(HANDLE_ENTRY_PARAM_PRE_ENTRY),
            Some(HOST_OWNED_STATE_POST_RUN),
        )
    } else if id == WIRE_ENTRY_PARAM_ID {
        (Some(WIRE_ENTRY_PARAM_PRE_ENTRY), None)
    } else {
        (None, None)
    }
}

fn run_ship_corpus_entry(
    id: &str,
    sources: &[subscript_compiler::SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    if sources
        .iter()
        .any(|f| f.source.lines().any(|l| l == "// file-provider: scratch"))
    {
        let fixture = file_fixture::Fixture::new();
        let libs = [fixture.library()];
        let mut config =
            subscript_codegen::RunConfig::default().with_enabled_modules(&["node:fs/promises"]);
        config.native_libraries = &libs;
        config.pre_init_hook = Some("subscript_test_files_setup");
        return subscript_codegen::run_c_aot_configured(sources, config).map(|o| o.stdout);
    }
    let (pre_entry, post_run) = host_hooks(id);
    run_c_aot_with_native_libraries_and_host_hooks(sources, libraries, pre_entry, post_run)
}

fn run_dev_corpus_entry(
    id: &str,
    sources: &[subscript_compiler::SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    if sources
        .iter()
        .any(|f| f.source.lines().any(|l| l == "// file-provider: scratch"))
    {
        let mut fixture = file_fixture::Fixture::new();
        return subscript_codegen::run_jit_configured(sources, fixture.config()).map(|o| o.stdout);
    }
    if id == HOST_OWNED_STATE_ID {
        let fixture =
            native_fixture::fixture().expect("the caller excludes unavailable fixture entries");
        let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
        fixture.host_owned_state_pre_entry();
        let run = (|| {
            session.call_main()?;
            session.call_export("secondEntry")?;
            while session.async_pending() != 0 {
                session.async_step()?;
            }
            Ok(session.take_output())
        })();
        fixture.host_owned_state_post_run();
        return run;
    }
    if id == HANDLE_ENTRY_PARAM_ID {
        let fixture =
            native_fixture::fixture().expect("the caller excludes unavailable fixture entries");
        let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
        fixture.host_owned_state_pre_entry();
        let run = (|| {
            let state = fixture.host_owned_state_borrow_and_advance();
            session.call_export_with("adopt", &[EntryArg::Handle(state), EntryArg::I32(7)])?;
            session.call_main()?;
            Ok(session.take_output())
        })();
        fixture.host_owned_state_post_run();
        return run;
    }
    if id == WIRE_ENTRY_PARAM_ID {
        let mut session = ReloadSession::new_with_native_libraries(sources, libraries)?;
        session.call_export_with("configure", &[EntryArg::I32(23), EntryArg::I32(5)])?;
        session.call_main()?;
        return Ok(session.take_output());
    }
    run_jit_with_native_libraries(sources, libraries)
}

fn native_libraries(sources: &[subscript_compiler::SourceFile]) -> Option<Vec<NativeLibrary>> {
    if sources.iter().any(|source| {
        corpus::references_interop(&source.source)
            || source.source.contains("SubWireMode")
            || source.source.contains("SubBindTone")
    }) {
        Some(vec![native_fixture::fixture()?.library()])
    } else {
        Some(Vec::new())
    }
}

/// One golden entry of the sweep, with its position in corpus order.
struct SweepEntry {
    index: usize,
    id: String,
    golden: Vec<u8>,
    sources: Vec<subscript_compiler::SourceFile>,
}

/// What one entry contributes to the sweep's counts and failure list.
struct SweepOutcome {
    compared: bool,
    failures: Vec<String>,
}

fn sweep_libraries(entry: &SweepEntry) -> Vec<NativeLibrary> {
    native_libraries(&entry.sources).expect("the selection step drops every excluded entry")
}

/// True when the dev run of the entry has a native library or a file
/// provider. Such a run needs a single-threaded process (compiler.md
/// §190.1 rule 3), so it runs on this thread after the pool.
fn dev_run_needs_the_main_thread(entry: &SweepEntry) -> bool {
    !sweep_libraries(entry).is_empty()
        || entry
            .sources
            .iter()
            .any(|f| f.source.lines().any(|l| l == "// file-provider: scratch"))
}

/// The dev-JIT run of one entry, or its failure line.
fn dev_sweep_run(entry: &SweepEntry) -> Result<Vec<u8>, String> {
    run_dev_corpus_entry(&entry.id, &entry.sources, &sweep_libraries(entry)).map_err(|error| {
        let stdout = match &error {
            RunError::AbnormalTermination(termination) => Some(termination.stdout.as_slice()),
            _ => None,
        };
        dev_sweep_failure(&entry.id, &error, stdout)
    })
}

/// Formats the dev failure and any stdout retained after abnormal termination.
fn dev_sweep_failure(id: &str, error: &impl std::fmt::Display, stdout: Option<&[u8]>) -> String {
    let mut failure = format!("{id}: dev-JIT run failed: {error}");
    if let Some(stdout) = stdout {
        failure.push_str(&format!(
            "\n{id}: dev-JIT output before termination:\n{}",
            String::from_utf8_lossy(stdout)
        ));
    }
    failure
}

fn abnormal_termination_failure_retains_stdout() {
    let failure = dev_sweep_failure("control", &"abnormal termination", Some(b"x\n"));
    assert!(
        failure.contains("output before termination:\nx\n"),
        "retained stdout is missing: {failure}"
    );
}

/// The ship tier: emit C, compile at -O2 -ffp-contract=off, link with
/// the runtime, run, capture stdout.
fn ship_sweep_run(entry: &SweepEntry) -> Result<Vec<u8>, String> {
    run_ship_corpus_entry(&entry.id, &entry.sources, &sweep_libraries(entry))
        .map_err(|e| format!("{}: ship-C-AOT run failed: {e}", entry.id))
}

/// Compares the two tier outputs of one entry and the golden. The
/// caller prints and asserts.
fn compare_sweep_outputs(
    entry: &SweepEntry,
    jit: Result<Vec<u8>, String>,
    ship: Result<Vec<u8>, String>,
) -> SweepOutcome {
    let id = &entry.id;
    let golden = &entry.golden;
    let mut failures = Vec::new();
    let (jit, ship) = match (jit, ship) {
        (Ok(jit), Ok(ship)) => (jit, ship),
        (Err(failure), _) | (Ok(_), Err(failure)) => {
            failures.push(failure);
            return SweepOutcome {
                compared: false,
                failures,
            };
        }
    };
    if jit != ship {
        failures.push(format!(
            "{id}: dev-JIT output {:?} != ship-C-AOT output {:?}",
            String::from_utf8_lossy(&jit),
            String::from_utf8_lossy(&ship)
        ));
    }
    if jit != *golden {
        failures.push(format!(
            "{id}: dev-JIT output {:?} != golden {:?}",
            String::from_utf8_lossy(&jit),
            String::from_utf8_lossy(golden)
        ));
    }
    if ship != *golden {
        failures.push(format!(
            "{id}: ship-C-AOT output {:?} != golden {:?}",
            String::from_utf8_lossy(&ship),
            String::from_utf8_lossy(golden)
        ));
    }
    SweepOutcome {
        compared: true,
        failures,
    }
}

/// Runs one entry on both tiers on this thread and compares them.
fn compare_sweep_entry(entry: &SweepEntry) -> SweepOutcome {
    compare_sweep_outputs(entry, dev_sweep_run(entry), ship_sweep_run(entry))
}

fn jit_ship_c_aot_and_golden_agree_byte_for_byte() {
    let accept = corpus::corpus_accept();
    let golden_ids = corpus::golden_ids(&accept);
    let mut skipped = 0usize;
    // An entry with host hooks drives the interop fixture's process-global
    // C state (`codegen/tests/support/native_fixture.rs`), so it runs on
    // this thread after the pool. Every other entry takes the plain dev
    // path and goes to the pool.
    let mut pooled = Vec::new();
    let mut hooked = Vec::new();
    for (index, id) in golden_ids.iter().enumerate() {
        let golden = corpus::golden_bytes(&accept, id);
        let sources = corpus::entry_sources(&accept, id);
        if native_libraries(&sources).is_none() {
            println!("{id}: skipped: interop fixture excluded here (compiler.md §11c)");
            skipped += 1;
            continue;
        }
        let entry = SweepEntry {
            index,
            id: id.clone(),
            golden,
            sources,
        };
        if matches!(host_hooks(id), (None, None)) {
            pooled.push(entry);
        } else {
            hooked.push(entry);
        }
    }

    // The pool runs the ship tier of every pooled entry and the dev tier
    // of each entry that can run from a pool thread. The other dev runs
    // happen on this thread after the pool joins its threads.
    let pooled_runs = pool::map_in_order(&pooled, |entry| {
        let jit = (!dev_run_needs_the_main_thread(entry)).then(|| dev_sweep_run(entry));
        (jit, ship_sweep_run(entry))
    });
    let mut outcomes: Vec<(usize, SweepOutcome)> = pooled
        .iter()
        .zip(pooled_runs)
        .map(|(entry, (jit, ship))| {
            let jit = jit.unwrap_or_else(|| dev_sweep_run(entry));
            (entry.index, compare_sweep_outputs(entry, jit, ship))
        })
        .collect();
    outcomes.extend(
        hooked
            .iter()
            .map(|entry| (entry.index, compare_sweep_entry(entry))),
    );
    outcomes.sort_by_key(|(index, _)| *index);

    let mut compared = 0usize;
    let mut failures = Vec::new();
    for (_, outcome) in outcomes {
        if outcome.compared {
            compared += 1;
        }
        failures.extend(outcome.failures);
    }
    println!("golden sweep: compared {compared} entries, skipped {skipped} entries");
    assert!(
        failures.is_empty(),
        "{} differential failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        compared + skipped,
        golden_ids.len(),
        "every committed golden must be compared on both tiers or explicitly skipped"
    );
    #[cfg(not(all(windows, target_env = "msvc")))]
    assert_eq!(
        skipped, 0,
        "the reference configuration compares every committed golden and skips none"
    );
}

fn every_corpus_entry_with_a_golden_ends_in_a_newline() {
    // Output shape is part of the corpus convention: every run-set
    // non-empty output ends at a line boundary. The retired r267 program is silent.
    let accept = corpus::corpus_accept();
    for id in corpus::golden_ids(&accept) {
        let golden = corpus::golden_bytes(&accept, &id);
        if golden.is_empty() {
            continue;
        }
        assert_eq!(
            golden.last(),
            Some(&b'\n'),
            "{id}: golden has no final newline"
        );
    }
}

// compiler.md §190.1 rule 3: phase 1 runs in parallel; each phase 2 test
// starts a dev run with a native library or a file provider and runs in
// its own process, on that process's main thread.
fn main() -> std::process::ExitCode {
    main_thread::run(&main_thread_tests![
        parallel: [
            abnormal_termination_failure_retains_stdout,
            every_corpus_entry_with_a_golden_ends_in_a_newline,
        ],
        main_thread: [
            jit_ship_c_aot_and_golden_agree_byte_for_byte,
        ],
    ])
}
