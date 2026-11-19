//! Early syntax errors remain deferred until the checker succeeds (compiler.md §341 rule 4c).
use oxc_ast::ast;

use super::{position, SourcePosition};
use crate::diag::Pos;

pub(super) fn ignored(message: &str, dts: bool) -> bool {
    matches!(
        message,
        "'using' declaration cannot appear in the bare case statement."
            | "'await using' declaration cannot appear in the bare case statement."
    ) || (dts && message == "Missing initializer in const declaration")
}

pub(super) fn check(
    program: &ast::Program<'_>,
    source: &SourcePosition,
    dts: bool,
) -> Vec<(String, Pos)> {
    oxc_semantic::SemanticBuilder::new()
        .with_check_syntax_error(true)
        .build(program)
        .diagnostics
        .iter()
        .filter(|error| !ignored(error.message.as_ref(), dts))
        .map(|error| {
            let offset = error.labels.first().map_or(0, |label| label.offset());
            (
                error.message.to_string(),
                position(source, source.base + offset),
            )
        })
        .collect()
}
