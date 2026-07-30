//! Testable state transitions for `subscript run --watch`.
//!
//! Polling and terminal I/O belong to the binary-facing command loop. This
//! module owns only the reload state: it checks one freshly loaded source set,
//! applies the warning policy, delegates swaps to [`ReloadSession`], and calls
//! `main` after a start or accepted swap.

use subscript_codegen::{ReloadError, ReloadSession, RunError, TrapReport};
use subscript_compiler::{check_program, check_warnings, Diagnostic, SourceFile, Warning};

/// The result of one successful call to the watched program's `main`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct WatchCall {
    /// Program stdout produced by the call.
    ///
    /// This is empty when `trap` is present, matching non-watch `run`, which
    /// reports the trap without forwarding partial output from the failed run.
    pub output: Vec<u8>,
    /// A trap that ended this call without ending the reload session.
    pub trap: Option<TrapReport>,
}

/// What happened when one freshly loaded source set was considered.
#[derive(Debug)]
#[non_exhaustive]
pub enum WatchOutcome {
    /// The source contents and loaded file set were identical to the last
    /// attempted cycle.
    Unchanged,
    /// No program was running before this step, and a session was started.
    Started(WatchCall),
    /// The live session accepted the new bodies and invoked `main`.
    Swapped(WatchCall),
    /// The program did not check, or warnings were denied.
    WaitingForFix,
    /// `ReloadSession` refused a declaration change and named it.
    Refused {
        /// The first changed declaration.
        declaration: String,
    },
    /// A backend or unsupported-program error prevented this cycle.
    Failed {
        /// The error text to render on stderr.
        message: String,
    },
}

/// Complete report from one watch step.
#[derive(Debug)]
#[non_exhaustive]
pub struct WatchStep {
    /// The transition taken by the watch state machine.
    pub outcome: WatchOutcome,
    /// Checker diagnostics for an edited program that was not accepted.
    pub diagnostics: Vec<Diagnostic>,
    /// Warnings for a clean program, whether accepted or denied.
    pub warnings: Vec<Warning>,
}

impl WatchStep {
    fn outcome(outcome: WatchOutcome, warnings: Vec<Warning>) -> Self {
        Self {
            outcome,
            diagnostics: Vec::new(),
            warnings,
        }
    }

    fn diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            outcome: WatchOutcome::WaitingForFix,
            diagnostics,
            warnings: Vec::new(),
        }
    }
}

/// A reload-capable watch state with no polling or process I/O.
///
/// Reusing one value across calls preserves the live [`ReloadSession`] and
/// records the last attempted loaded source set. Tests can therefore exercise
/// the complete state machine without spawning a process or sleeping.
#[derive(Debug)]
pub struct WatchSession {
    session: Option<ReloadSession>,
    last_sources: Option<Vec<SourceFile>>,
    deny_warnings: bool,
}

impl WatchSession {
    /// Creates an empty watch state.
    #[must_use]
    pub fn new(deny_warnings: bool) -> Self {
        Self {
            session: None,
            last_sources: None,
            deny_warnings,
        }
    }

    /// Forgets the last attempted source set after an on-disk load failure.
    ///
    /// The live program is retained. The next successfully loaded set will be
    /// reconsidered even if it is byte-identical to the last successful load.
    pub fn invalidate_loaded_sources(&mut self) {
        self.last_sources = None;
    }

    /// Checks and applies one freshly loaded source set.
    ///
    /// Diagnostics and denied warnings never reach `ReloadSession`. Clean
    /// edits are passed to [`ReloadSession::reload`], which remains the sole
    /// authority for declaration compatibility and swap semantics.
    pub fn step(&mut self, files: &[SourceFile]) -> WatchStep {
        if self.last_sources.as_deref() == Some(files) {
            return WatchStep::outcome(WatchOutcome::Unchanged, Vec::new());
        }
        self.last_sources = Some(files.to_vec());

        let module = match check_program(files) {
            Ok(module) => module,
            Err(diagnostics) => return WatchStep::diagnostics(diagnostics),
        };
        let warnings = check_warnings(&module);
        if self.deny_warnings && !warnings.is_empty() {
            return WatchStep::outcome(WatchOutcome::WaitingForFix, warnings);
        }

        if self.session.is_none() {
            return self.start(files, warnings);
        }
        self.reload(files, warnings)
    }

    fn start(&mut self, files: &[SourceFile], warnings: Vec<Warning>) -> WatchStep {
        match ReloadSession::new_capturing_initializer_trap(files) {
            Ok((mut session, Some(trap))) => {
                // Non-watch `run` does not forward output from a trapped run.
                let _ = session.take_output();
                self.session = Some(session);
                WatchStep::outcome(
                    WatchOutcome::Started(WatchCall {
                        output: Vec::new(),
                        trap: Some(trap),
                    }),
                    warnings,
                )
            }
            Ok((mut session, None)) => {
                let call = call_main(&mut session);
                self.session = Some(session);
                match call {
                    Ok(call) => WatchStep::outcome(WatchOutcome::Started(call), warnings),
                    Err(message) => WatchStep::outcome(WatchOutcome::Failed { message }, warnings),
                }
            }
            Err(RunError::Rejected(diagnostics)) => WatchStep::diagnostics(diagnostics),
            Err(error) => WatchStep::outcome(
                WatchOutcome::Failed {
                    message: error.to_string(),
                },
                warnings,
            ),
        }
    }

    fn reload(&mut self, files: &[SourceFile], warnings: Vec<Warning>) -> WatchStep {
        let session = self
            .session
            .as_mut()
            .expect("reload path requires an existing session");
        match session.reload(files) {
            Ok(()) => match call_main(session) {
                Ok(call) => WatchStep::outcome(WatchOutcome::Swapped(call), warnings),
                Err(message) => WatchStep::outcome(WatchOutcome::Failed { message }, warnings),
            },
            Err(ReloadError::Rejected(diagnostics)) => WatchStep::diagnostics(diagnostics),
            Err(ReloadError::DeclarationChanged { declaration }) => {
                WatchStep::outcome(WatchOutcome::Refused { declaration }, warnings)
            }
            Err(error) => WatchStep::outcome(
                WatchOutcome::Failed {
                    message: error.to_string(),
                },
                warnings,
            ),
        }
    }
}

fn call_main(session: &mut ReloadSession) -> Result<WatchCall, String> {
    match session.call_main() {
        Ok(()) => Ok(WatchCall {
            output: session.take_output(),
            trap: None,
        }),
        Err(RunError::Trap(trap)) => {
            // Match non-watch `run`: the trap is rendered, while partial
            // stdout from the failed invocation is discarded.
            let _ = session.take_output();
            Ok(WatchCall {
                output: Vec::new(),
                trap: Some(trap),
            })
        }
        Err(error) => {
            let _ = session.take_output();
            Err(error.to_string())
        }
    }
}
