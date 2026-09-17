//! Stable, rustc-shaped diagnostic rendering.

use std::fmt::Write as _;

use crate::divergence::Divergence;
use crate::{Diagnostic, Pos, SourceFile, Warning};

/// The most bytes of a source line that one snippet carries (§109.2).
const WINDOW_BYTES: usize = 240;

/// The bytes of the window on each side of the column (§109.2).
const WINDOW_HALF: usize = WINDOW_BYTES / 2;

/// The marker that stands for the bytes a window cut away (§109.2).
const CUT: &str = "…";

/// The most items that one render writes (§109.2). The summary line
/// carries the total.
const MAX_ITEMS: usize = 200;

struct RenderItem<'a> {
    code: &'static str,
    message: &'a str,
    pos: &'a Pos,
    explanation: &'static str,
    divergence: Option<Divergence>,
}

/// The byte offset where each line of one source starts.
///
/// §109.2: the renderer indexes the lines of a file once. A scan from
/// byte zero for each item is quadratic in the item count.
struct LineIndex<'a> {
    source: &'a str,
    /// The start of each `\n`-separated segment, the empty last one
    /// included.
    starts: Vec<usize>,
    /// The number of lines that `str::lines` answers.
    count: usize,
}

impl<'a> LineIndex<'a> {
    /// Indexes the lines of one source in a single pass.
    fn new(source: &'a str) -> Self {
        let mut starts = vec![0usize];
        starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
        let count = if source.is_empty() {
            0
        } else if source.ends_with('\n') {
            starts.len() - 1
        } else {
            starts.len()
        };
        LineIndex {
            source,
            starts,
            count,
        }
    }

    /// Answers the 0-based line, as `str::lines` answers it.
    ///
    /// A line outside the source answers `None`.
    fn line(&self, index: usize) -> Option<&'a str> {
        if index >= self.count {
            return None;
        }
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.source.len(), |next| next - 1);
        let line = self.source.get(start..end)?;
        Some(line.strip_suffix('\r').unwrap_or(line))
    }
}

/// One rendered source line: the window text and the caret padding.
struct Snippet {
    /// The window, with `…` at each edge that cut bytes away.
    text: String,
    /// The columns between the start of the text and the caret.
    caret: usize,
}

/// Builds the window of at most [`WINDOW_BYTES`] bytes around the
/// column (§109.2).
///
/// The column is a 1-based character count, so the window holds the
/// bytes from [`WINDOW_HALF`] before that character to [`WINDOW_HALF`]
/// after it. Each edge moves inwards to a UTF-8 boundary, which keeps
/// the window under the limit. The caret padding is the column's
/// offset inside the text, the `…` marker included.
fn snippet(line: &str, col: u32) -> Snippet {
    let column = usize::try_from(col.saturating_sub(1)).unwrap_or(usize::MAX);
    let focus = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(offset, _)| offset);
    let mut start = focus.saturating_sub(WINDOW_HALF);
    let mut end = focus.saturating_add(WINDOW_HALF).min(line.len());
    while start < focus && !line.is_char_boundary(start) {
        start += 1;
    }
    while end > focus && !line.is_char_boundary(end) {
        end -= 1;
    }
    let window = line.get(start..end).unwrap_or("");
    let mut text = String::with_capacity(window.len() + 2 * CUT.len());
    if start > 0 {
        text.push_str(CUT);
    }
    text.push_str(window);
    if end < line.len() {
        text.push_str(CUT);
    }
    let caret = line.get(start..focus).unwrap_or("").chars().count() + usize::from(start > 0);
    Snippet { text, caret }
}

/// Renders diagnostics against their source files without ANSI color.
///
/// The returned string has no trailing newline. A diagnostic whose file
/// cannot be found, or whose 1-based line is outside that file, degrades to
/// its `error[...]` header and location line without a source snippet.
/// Columns are rendered as character counts; tabs and earlier multi-byte
/// characters can therefore shift visual alignment. §109.2 bounds the
/// output: a snippet carries at most [`WINDOW_BYTES`] bytes of its line
/// around the column, and the render stops after [`MAX_ITEMS`] items.
/// The summary line carries the total count.
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
/// snippet, caret, degradation, bounds, and summary shape as
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
    let indexes = files
        .iter()
        .map(|file| LineIndex::new(&file.source))
        .collect::<Vec<_>>();
    let gutter_width = items
        .iter()
        .filter(|item| source_line(files, &indexes, item.pos).is_some())
        .map(|item| item.pos.line.to_string().len())
        .max()
        .unwrap_or(1);
    let mut rendered = String::new();

    for item in items.iter().take(MAX_ITEMS) {
        let _ = writeln!(rendered, "{severity}[{}]: {}", item.code, item.message);
        let _ = writeln!(
            rendered,
            " --> {}:{}:{}",
            item.pos.file, item.pos.line, item.pos.col
        );

        let Some(line) = source_line(files, &indexes, item.pos) else {
            continue;
        };
        let snippet = snippet(line, item.pos.col);
        let gutter_padding = " ".repeat(gutter_width + 1);
        let caret_padding = " ".repeat(snippet.caret);
        let _ = writeln!(rendered, "{gutter_padding}|");
        let _ = writeln!(
            rendered,
            "{:>width$} | {}",
            item.pos.line,
            snippet.text,
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

fn source_line<'a>(
    files: &[SourceFile],
    indexes: &'a [LineIndex<'a>],
    pos: &Pos,
) -> Option<&'a str> {
    let index = pos.line.checked_sub(1)?;
    let index = usize::try_from(index).ok()?;
    let position = files.iter().position(|file| file.name == pos.file)?;
    indexes.get(position)?.line(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pos, RuleCode, WarnCode, Warning};

    /// The snippet line of the first rendered item, without its gutter.
    fn snippet_line(rendered: &str) -> &str {
        rendered
            .lines()
            .nth(3)
            .and_then(|line| line.split_once(" | "))
            .expect("the rendered form carries a snippet line")
            .1
    }

    /// The caret line of the first rendered item, without its gutter.
    fn caret_line(rendered: &str) -> &str {
        rendered
            .lines()
            .nth(4)
            .and_then(|line| line.split_once("| "))
            .expect("the rendered form carries a caret line")
            .1
    }

    #[test]
    fn the_line_index_answers_what_str_lines_answers() {
        let sources = ["one\ntwo\nthree\n", "alpha\nbeta", "\r\ncarriage\r\n\nlast"];
        for source in sources {
            let index = LineIndex::new(source);
            for line in 0..source.lines().count() + 3 {
                assert_eq!(
                    index.line(line),
                    source.lines().nth(line),
                    "line {line} of {source:?}"
                );
            }
        }
    }

    #[test]
    fn a_long_line_renders_a_240_byte_window_with_both_cuts() {
        let files = [SourceFile::new(
            "main.ts",
            format!("{}\n", "z".repeat(10_000)),
        )];
        let diagnostics = [Diagnostic::new(
            RuleCode::S100,
            "far inside the line",
            Pos::new("main.ts", 1, 5_000),
        )];

        let rendered = render_diagnostics(&files, &diagnostics);
        let snippet = snippet_line(&rendered);
        assert_eq!(snippet, format!("…{}…", "z".repeat(240)));
        assert_eq!(caret_line(&rendered), format!("{}^", " ".repeat(121)));
    }

    #[test]
    fn a_column_near_the_start_has_no_left_cut() {
        let files = [SourceFile::new(
            "main.ts",
            format!("{}\n", "z".repeat(10_000)),
        )];
        let diagnostics = [Diagnostic::new(
            RuleCode::S100,
            "near the start",
            Pos::new("main.ts", 1, 3),
        )];

        let rendered = render_diagnostics(&files, &diagnostics);
        assert_eq!(snippet_line(&rendered), format!("{}…", "z".repeat(122)));
        assert_eq!(caret_line(&rendered), format!("{}^", " ".repeat(2)));
    }

    #[test]
    fn a_cut_inside_a_multibyte_character_moves_to_the_boundary() {
        // 10 bytes, then a two-byte character at bytes 10 and 11, then
        // 300 bytes. The three columns put the left cut on the
        // character, one byte inside it, and one byte after it.
        let line = format!("{}é{}", "x".repeat(10), "y".repeat(300));
        let files = [SourceFile::new("main.ts", format!("{line}\n"))];
        for (col, window, caret) in [
            (130, format!("é{}", "y".repeat(238)), 120),
            (131, "y".repeat(239), 120),
            (132, "y".repeat(240), 121),
        ] {
            let diagnostics = [Diagnostic::new(
                RuleCode::S100,
                "at the cut",
                Pos::new("main.ts", 1, col),
            )];
            let rendered = render_diagnostics(&files, &diagnostics);
            assert_eq!(
                snippet_line(&rendered),
                format!("…{window}…"),
                "column {col}"
            );
            assert_eq!(
                caret_line(&rendered),
                format!("{}^", " ".repeat(caret)),
                "column {col}"
            );
        }
    }

    #[test]
    fn the_render_stops_at_200_items_and_the_summary_carries_the_total() {
        let source = "const value: number = 1;\n".repeat(201);
        let files = [SourceFile::new("main.ts", source)];
        let diagnostics = (0..201)
            .map(|line| {
                Diagnostic::new(
                    RuleCode::S007,
                    "bare number",
                    Pos::new("main.ts", line + 1, 14),
                )
            })
            .collect::<Vec<_>>();

        let rendered = render_diagnostics(&files, &diagnostics);
        assert_eq!(rendered.matches("error[S007]").count(), 200);
        assert_eq!(rendered.matches("const value: number = 1;").count(), 200);
        assert!(rendered.contains(" --> main.ts:200:14"));
        assert!(!rendered.contains(" --> main.ts:201:14"));
        assert!(rendered.ends_with("error: 201 error(s)"));
    }

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
