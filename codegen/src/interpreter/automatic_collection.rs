//! An interpreter run with a collection mode (`compiler.md` §342 rule 17).

use super::*;

/// The collection counters of an interpreter run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CollectionRun {
    /// The number of automatic collections (`compiler.md` §342 rule 7).
    pub automatic_collections: u64,
    /// The number of detection collections (`compiler.md` §342 rule 15).
    pub detection_collections: u64,
    /// The number of outermost exits (`compiler.md` §342 rule 16).
    pub outermost_exits: u64,
    /// Live bytes when the run ends, before the Context is released.
    pub live_bytes: usize,
}

/// Executes a module as [`interpret`] does, with the collection mode and
/// the minimum headroom set on the Context before the initializer runs.
///
/// # Errors
///
/// Returns the errors of [`interpret`]. A mode change that the Context
/// refuses is an [`InterpretError::InvalidLir`] error.
pub fn interpret_with_collection(
    module: &l::Module,
    mode: subscript_runtime::context::CollectionMode,
    min_headroom_bytes: usize,
) -> Result<(Vec<u8>, CollectionRun), InterpretError> {
    let config = InterpretConfig {
        collection: Some((mode, min_headroom_bytes)),
        ..InterpretConfig::default()
    };
    interpret_configured(module, config)
}

/// The Context options of an interpreter run, set before the initializer
/// runs.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct InterpretConfig {
    /// The collection mode and the minimum headroom in bytes
    /// (`compiler.md` §342 rule 17). `None` keeps the Context default.
    pub collection: Option<(subscript_runtime::context::CollectionMode, usize)>,
    /// Retains each freed block, as the dev JIT does with the same option
    /// (`compiler.md` §8.1a). A freed handle then stays not live, so the
    /// `DevOnlyRelease` and `DevOnlyLifetime` checks of `compiler.md`
    /// §358 rule 7 do not depend on the reuse of an address.
    pub freed_handle_diagnostics: bool,
}

/// Executes a module as [`interpret`] does, with the Context options of
/// `config`.
///
/// # Errors
///
/// Returns the errors of [`interpret`]. An option that the Context
/// refuses is an [`InterpretError::InvalidLir`] error.
pub fn interpret_configured(
    module: &l::Module,
    config: InterpretConfig,
) -> Result<(Vec<u8>, CollectionRun), InterpretError> {
    let mut interpreter = Interpreter::new(module)?;
    if config.freed_handle_diagnostics
        && !interpreter.context.set_freed_handle_diagnostics(
            true,
            0,
            subscript_runtime::FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES,
        )
    {
        return Err(interpreter.invalid(None, "the Context refused freed-handle diagnostics"));
    }
    if let Some((mode, min_headroom_bytes)) = config.collection {
        if !interpreter.context.set_collection(mode, min_headroom_bytes) {
            return Err(interpreter.invalid(None, "the Context refused the collection mode"));
        }
    }
    match interpreter.run() {
        Ok(output) => Ok((
            output,
            CollectionRun {
                automatic_collections: interpreter.context.automatic_collection_count(),
                detection_collections: interpreter.context.detection_collection_count(),
                outermost_exits: interpreter.context.outermost_exit_count(),
                live_bytes: interpreter.context.live_bytes(),
            },
        )),
        Err(source) => Err(InterpretError::Execution {
            output: interpreter.context.take_stdout(),
            source: Box::new(source),
        }),
    }
}
