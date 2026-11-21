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
    let mut interpreter = Interpreter::new(module)?;
    if !interpreter.context.set_collection(mode, min_headroom_bytes) {
        return Err(interpreter.invalid(None, "the Context refused the collection mode"));
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
