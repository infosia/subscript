//! The sandbox-profile interrupt, one test per tier
//! (`specs/blocks/compiler.md` §109.7).
//!
//! The corpus has no second thread, so the interrupt has no corpus entry.
//! Each test here runs a profile program whose `main` prints one line and
//! then loops forever, sets the Context interrupt flag from a second
//! thread after 50 ms, and asserts that the run returns with the
//! `interrupted` trap and that one line.
//!
//! The firing control is the same program with no second thread. It must
//! not complete inside a 2 s bound, and each control asserts that the
//! bound fired. The dev-JIT control then sets the flag through the
//! Context's interrupt handle and joins its runner thread, so no thread
//! outlives the test.
//!
//! The reference interpreter has no test here: `interpret_configured`
//! owns its Context for the whole call and hands out no interrupt
//! handle, so this file cannot set the flag on an interpreter run.

use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use subscript_codegen::{run_c_aot_interrupted, run_jit_interrupted, RunConfig, RunError};
use subscript_compiler::{Profile, SourceFile};
use subscript_runtime::{Interrupt, TrapKind};

/// The delay the second thread sleeps before it sets the flag.
const INTERRUPT_AFTER_MILLIS: u64 = 50;

/// The bound each firing control runs under.
const CONTROL_BOUND: Duration = Duration::from_secs(2);

/// The bound an interrupted run returns inside.
const INTERRUPTED_BOUND: Duration = Duration::from_secs(120);

/// One printed line, then an endless loop. `spin` never passes
/// 1,000,000, so the condition never turns false and no overflow occurs.
const ENDLESS: &str = "\
export function main(): void {\n\
\x20 print(\"running\");\n\
\x20 let spin: i32 = 0;\n\
\x20 while (spin >= 0) {\n\
\x20   spin = spin + 1;\n\
\x20   if (spin > 1000000) {\n\
\x20     spin = 1;\n\
\x20   }\n\
\x20 }\n\
}\n";

fn endless_program() -> Vec<SourceFile> {
    vec![SourceFile::new("endless.ts", ENDLESS)]
}

/// Asserts that `outcome` is the interrupt trap with the one printed line.
fn assert_interrupted(tier: &str, outcome: &Result<Vec<u8>, RunError>) {
    match outcome {
        Err(RunError::Trap(report)) => {
            assert_eq!(report.rule, TrapKind::Interrupted, "{tier}");
            assert_eq!(
                report.stdout,
                b"running\n".to_vec(),
                "{tier}: the pre-trap output is the one printed line"
            );
        }
        Ok(stdout) => panic!(
            "{tier}: the endless program completed with {:?}",
            String::from_utf8_lossy(stdout)
        ),
        Err(error) => panic!("{tier}: expected the interrupt trap, got {error}"),
    }
}

#[test]
fn the_dev_jit_returns_the_interrupt_trap() {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let files = endless_program();
        let config = RunConfig::with_profile(Profile::Sandbox)
            .with_interrupt_after_millis(INTERRUPT_AFTER_MILLIS);
        let _ = sender.send(run_jit_interrupted(&files, config));
    });
    let (outcome, latency) = receiver
        .recv_timeout(INTERRUPTED_BOUND)
        .expect("the interrupted dev-JIT run returns");
    assert_interrupted("dev-JIT", &outcome);
    let latency = latency.expect("the dev-JIT run measured its interrupt latency");
    println!("interrupt latency dev-JIT: {} ns", latency.as_nanos());
}

/// The bound the control waits for its runner thread to return in, after
/// it sets the flag.
const CONTROL_TEARDOWN_BOUND: Duration = Duration::from_secs(60);

/// How long the control polls for the runner to store its handle.
const HANDLE_BOUND: Duration = Duration::from_secs(10);

/// Reads the handle the runner stored, polling until `HANDLE_BOUND`.
fn await_handle(sink: &OnceLock<Arc<Interrupt>>) -> Arc<Interrupt> {
    let started = Instant::now();
    loop {
        if let Some(handle) = sink.get() {
            return Arc::clone(handle);
        }
        assert!(
            started.elapsed() < HANDLE_BOUND,
            "the dev-JIT runner stored no interrupt handle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_dev_jit_endless_program_does_not_stop_without_the_interrupt() {
    let sink: Arc<OnceLock<Arc<Interrupt>>> = Arc::new(OnceLock::new());
    let runner_sink = Arc::clone(&sink);
    let (sender, receiver) = mpsc::channel();
    let runner = std::thread::spawn(move || {
        let files = endless_program();
        let config = RunConfig::with_profile(Profile::Sandbox).with_interrupt_handle(&runner_sink);
        let _ = sender.send(run_jit_interrupted(&files, config));
    });
    let outcome = receiver.recv_timeout(CONTROL_BOUND);
    assert!(
        outcome.is_err(),
        "the control completed, so the interrupt test can pass without the flag: {:?}",
        outcome.map(|(run, _)| run.is_ok())
    );
    println!("dev-JIT control: the {CONTROL_BOUND:?} bound fired");

    // §109.4 rule 1: the handle stops the run the control started, so no
    // thread outlives this test.
    await_handle(&sink).set();
    let stopped = receiver
        .recv_timeout(CONTROL_TEARDOWN_BOUND)
        .expect("the control run returns once the flag is set");
    assert_interrupted("dev-JIT control", &stopped.0);
    runner.join().expect("the control runner thread");
}

#[test]
fn the_ship_tier_returns_the_interrupt_trap() {
    let files = endless_program();
    let config = RunConfig::with_profile(Profile::Sandbox)
        .with_interrupt_after_millis(INTERRUPT_AFTER_MILLIS);
    let finished = run_c_aot_interrupted(&files, config, INTERRUPTED_BOUND)
        .expect("the ship-tier build succeeds")
        .expect("the interrupted ship-C-AOT run returns");
    assert_interrupted("ship-C-AOT", &finished.outcome);
    // The linked program measures the span itself, from the flag store to
    // the end of its run, and reports it on stderr.
    let latency = finished
        .latency
        .expect("the linked program reported its interrupt latency");
    println!("interrupt latency ship-C-AOT: {} ns", latency.as_nanos());
}

#[test]
fn the_ship_tier_endless_program_does_not_stop_without_the_interrupt() {
    let files = endless_program();
    let outcome = run_c_aot_interrupted(
        &files,
        RunConfig::with_profile(Profile::Sandbox),
        CONTROL_BOUND,
    )
    .expect("the ship-tier build succeeds");
    assert!(
        outcome.is_none(),
        "the control completed, so the interrupt test can pass without the flag"
    );
    println!("ship-C-AOT control: the {CONTROL_BOUND:?} bound fired");
}
