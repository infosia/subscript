//! Stable, rustc-shaped diagnostic rendering.

use std::fmt::Write as _;

use crate::{Diagnostic, SourceFile};

/// Renders diagnostics against their source files without ANSI color.
///
/// The returned string has no trailing newline. A diagnostic whose file
/// cannot be found, or whose 1-based line is outside that file, degrades to
/// its `error[...]` header and location line without a source snippet.
/// Columns are rendered as character counts; tabs and earlier multi-byte
/// characters can therefore shift visual alignment.
#[must_use]
pub fn render_diagnostics(files: &[SourceFile], diagnostics: &[Diagnostic]) -> String {
    let gutter_width = diagnostics
        .iter()
        .filter(|diagnostic| source_line(files, diagnostic).is_some())
        .map(|diagnostic| diagnostic.pos.line.to_string().len())
        .max()
        .unwrap_or(1);
    let mut rendered = String::new();

    for diagnostic in diagnostics {
        let _ = writeln!(
            rendered,
            "error[{}]: {}",
            diagnostic.code, diagnostic.message
        );
        let _ = writeln!(
            rendered,
            " --> {}:{}:{}",
            diagnostic.pos.file, diagnostic.pos.line, diagnostic.pos.col
        );

        let Some(line) = source_line(files, diagnostic) else {
            continue;
        };
        let gutter_padding = " ".repeat(gutter_width + 1);
        let caret_padding = " ".repeat(diagnostic.pos.col.saturating_sub(1) as usize);
        let _ = writeln!(rendered, "{gutter_padding}|");
        let _ = writeln!(
            rendered,
            "{:>width$} | {line}",
            diagnostic.pos.line,
            width = gutter_width
        );
        let _ = writeln!(rendered, "{gutter_padding}| {caret_padding}^");
        let _ = writeln!(
            rendered,
            "{gutter_padding}= rule: {}",
            diagnostic.code.explanation()
        );
    }

    let _ = write!(rendered, "error: {} error(s)", diagnostics.len());
    rendered
}

fn source_line<'a>(files: &'a [SourceFile], diagnostic: &Diagnostic) -> Option<&'a str> {
    let index = diagnostic.pos.line.checked_sub(1)?;
    let index = usize::try_from(index).ok()?;
    files
        .iter()
        .find(|file| file.name == diagnostic.pos.file)?
        .source
        .lines()
        .nth(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pos, RuleCode};

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
}
