//! Stable, rustc-shaped diagnostic rendering.

use std::fmt::Write as _;

use crate::divergence::Divergence;
use crate::{Diagnostic, Pos, SourceFile, Warning};

struct RenderItem<'a> {
    code: &'static str,
    message: &'a str,
    pos: &'a Pos,
    explanation: &'static str,
    divergence: Option<Divergence>,
}

/// Renders diagnostics against their source files without ANSI color.
///
/// The returned string has no trailing newline. A diagnostic whose file
/// cannot be found, or whose 1-based line is outside that file, degrades to
/// its `error[...]` header and location line without a source snippet.
/// Columns are rendered as character counts; tabs and earlier multi-byte
/// characters can therefore shift visual alignment.
#[must_use]
pub fn render_diagnostics(files: &[SourceFile], diagnostics: &[Diagnostic]) -> String {
    let items = diagnostics
        .iter()
        .map(|diagnostic| RenderItem {
            code: diagnostic.code.as_str(),
            message: &diagnostic.message,
            pos: &diagnostic.pos,
            explanation: diagnostic.code.explanation(),
            divergence: diagnostic.divergence,
        })
        .collect::<Vec<_>>();
    render_items(files, &items, "error")
}

/// Renders warnings against their source files without ANSI color.
///
/// The returned string has no trailing newline and uses the same source
/// snippet, caret, degradation, and summary shape as
/// [`render_diagnostics`].
#[must_use]
pub fn render_warnings(files: &[SourceFile], warnings: &[Warning]) -> String {
    let items = warnings
        .iter()
        .map(|warning| RenderItem {
            code: warning.code.as_str(),
            message: &warning.message,
            pos: &warning.pos,
            explanation: warning.code.explanation(),
            divergence: None,
        })
        .collect::<Vec<_>>();
    render_items(files, &items, "warning")
}

fn render_items(files: &[SourceFile], items: &[RenderItem<'_>], severity: &str) -> String {
    let gutter_width = items
        .iter()
        .filter(|item| source_line(files, item.pos).is_some())
        .map(|item| item.pos.line.to_string().len())
        .max()
        .unwrap_or(1);
    let mut rendered = String::new();

    for item in items {
        let _ = writeln!(rendered, "{severity}[{}]: {}", item.code, item.message);
        let _ = writeln!(
            rendered,
            " --> {}:{}:{}",
            item.pos.file, item.pos.line, item.pos.col
        );

        let Some(line) = source_line(files, item.pos) else {
            continue;
        };
        let gutter_padding = " ".repeat(gutter_width + 1);
        let caret_padding = " ".repeat(item.pos.col.saturating_sub(1) as usize);
        let _ = writeln!(rendered, "{gutter_padding}|");
        let _ = writeln!(
            rendered,
            "{:>width$} | {line}",
            item.pos.line,
            width = gutter_width
        );
        let _ = writeln!(rendered, "{gutter_padding}| {caret_padding}^");
        let _ = writeln!(rendered, "{gutter_padding}= rule: {}", item.explanation);
        if let Some(divergence) = item.divergence {
            let entry = divergence.entry();
            let _ = writeln!(rendered, "{gutter_padding}= TypeScript accepts:");
            for line in entry.ts.lines() {
                let _ = writeln!(rendered, "{gutter_padding}|   {line}");
            }
            let _ = writeln!(rendered, "{gutter_padding}= subscript:");
            for line in entry.subscript.lines() {
                let _ = writeln!(rendered, "{gutter_padding}|   {line}");
            }
            let collision = if entry.collision.starts_with('C')
                && entry.collision[1..].chars().all(|c| c.is_ascii_digit())
            {
                format!("collisions.md {}", entry.collision)
            } else {
                entry.collision.to_string()
            };
            let _ = writeln!(
                rendered,
                "{gutter_padding}= why: {} ({collision})",
                entry.why
            );
        }
    }

    let _ = write!(rendered, "{severity}: {} {severity}(s)", items.len());
    rendered
}

fn source_line<'a>(files: &'a [SourceFile], pos: &Pos) -> Option<&'a str> {
    let index = pos.line.checked_sub(1)?;
    let index = usize::try_from(index).ok()?;
    files
        .iter()
        .find(|file| file.name == pos.file)?
        .source
        .lines()
        .nth(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pos, RuleCode, WarnCode, Warning};

    #[test]
    fn renders_the_divergence_block_exactly() {
        let files = [SourceFile::new("main.ts", "const value: number = 1;\n")];
        let mut diagnostic =
            Diagnostic::new(RuleCode::S007, "bare number", Pos::new("main.ts", 1, 14));
        diagnostic.divergence = Some(Divergence::BareNumber);

        assert_eq!(
            render_diagnostics(&files, &[diagnostic]),
            concat!(
                "error[S007]: bare number\n",
                " --> main.ts:1:14\n",
                "  |\n",
                "1 | const value: number = 1;\n",
                "  |              ^\n",
                "  = rule: Bare `number` is rejected; sized numeric types are mandatory.\n",
                "  = TypeScript accepts:\n",
                "  |   const count: number = 3;\n",
                "  = subscript:\n",
                "  |   const count: i32 = 3;\n",
                "  = why: `number` is a 64-bit float with no C width, so every declaration names one of the sized types. (collisions.md C3)\n",
                "error: 1 error(s)",
            )
        );
    }

    #[test]
    fn renders_one_diagnostic_with_snippet_and_caret_exactly() {
        let files = [SourceFile::new(
            "main.ts",
            "function noop(): void {}\n\nconst value: number = 1;\n",
        )];
        let diagnostics = [Diagnostic::new(
            RuleCode::S007,
            "bare number",
            Pos::new("main.ts", 3, 14),
        )];

        assert_eq!(
            render_diagnostics(&files, &diagnostics),
            concat!(
                "error[S007]: bare number\n",
                " --> main.ts:3:14\n",
                "  |\n",
                "3 | const value: number = 1;\n",
                "  |              ^\n",
                "  = rule: Bare `number` is rejected; sized numeric types are mandatory.\n",
                "error: 1 error(s)",
            )
        );
    }

    #[test]
    fn renders_multiple_diagnostics_with_one_gutter_width_and_count() {
        let files = [SourceFile::new(
            "main.ts",
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve\n",
        )];
        let diagnostics = [
            Diagnostic::new(RuleCode::S001, "first", Pos::new("main.ts", 2, 1)),
            Diagnostic::new(RuleCode::S012, "second", Pos::new("main.ts", 12, 4)),
        ];

        assert_eq!(
            render_diagnostics(&files, &diagnostics),
            concat!(
                "error[S001]: first\n",
                " --> main.ts:2:1\n",
                "   |\n",
                " 2 | two\n",
                "   | ^\n",
                "   = rule: `any` is not part of the language.\n",
                "error[S012]: second\n",
                " --> main.ts:12:4\n",
                "   |\n",
                "12 | twelve\n",
                "   |    ^\n",
                "   = rule: `undefined` is banned; the single null story is `null`.\n",
                "error: 2 error(s)",
            )
        );
    }

    #[test]
    fn unresolved_files_and_lines_degrade_without_a_snippet() {
        let files = [SourceFile::new("main.ts", "one line\n")];
        let diagnostics = [
            Diagnostic::new(RuleCode::S100, "missing file", Pos::new("missing.ts", 9, 4)),
            Diagnostic::new(RuleCode::S100, "missing line", Pos::new("main.ts", 2, 1)),
        ];

        assert_eq!(
            render_diagnostics(&files, &diagnostics),
            concat!(
                "error[S100]: missing file\n",
                " --> missing.ts:9:4\n",
                "error[S100]: missing line\n",
                " --> main.ts:2:1\n",
                "error: 2 error(s)",
            )
        );
    }

    #[test]
    fn warning_rendering_reuses_the_diagnostic_shape() {
        let files = [SourceFile::new("main.ts", "const token = allocate();\n")];
        let warnings = [Warning::new(
            WarnCode::W001,
            "allocation repeats",
            Pos::new("main.ts", 1, 15),
        )];

        assert_eq!(
            render_warnings(&files, &warnings),
            concat!(
                "warning[W001]: allocation repeats\n",
                " --> main.ts:1:15\n",
                "  |\n",
                "1 | const token = allocate();\n",
                "  |               ^\n",
                "  = rule: A reference-class allocation repeated by a loop should escape the iteration or be released.\n",
                "warning: 1 warning(s)",
            )
        );
    }
}
