//! §109.2 rule 5 total check: the parser's entry owns the S026 byte
//! check.
//!
//! A lexer or a parser that another site builds is a parser entry
//! without the check, and no review can find every such site. This test
//! reads every Rust source of `compiler/src` and `cli/src` and fails on
//! one construction outside `parse.rs`'s one lexer constructor.

use std::path::{Path, PathBuf};

/// The file that holds the one lexer constructor.
const PARSER_FILE: &str = "compiler/src/parse.rs";

/// The name of that constructor.
const CONSTRUCTOR: &str = "fn lexer_for";

/// The constructions this test reads.
const NEEDLES: [&str; 3] = ["Lexer::new(", "Parser::new_from(", "Parser::new("];

/// The repository root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the compiler crate has a parent directory")
        .to_path_buf()
}

/// Every `.rs` file under `directory`, in sorted order.
fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        let entries = std::fs::read_dir(&next).unwrap_or_else(|error| {
            panic!("read {}: {error}", next.display());
        });
        for entry in entries {
            let path = entry.expect("read a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every construction site in `text`, as a byte offset with its line.
fn sites(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for needle in NEEDLES {
        let mut at = 0;
        while let Some(offset) = text[at..].find(needle) {
            let start = at + offset;
            let line = text[..start].lines().count();
            found.push((start, line));
            at = start + needle.len();
        }
    }
    found.sort_unstable();
    found
}

/// The byte range of the one constructor's body in `text`.
///
/// The body ends at the first `}` in the first column after the
/// signature, which is what rustfmt writes for an item of a module.
fn constructor_range(text: &str) -> std::ops::Range<usize> {
    let start = text
        .find(CONSTRUCTOR)
        .unwrap_or_else(|| panic!("{PARSER_FILE} holds `{CONSTRUCTOR}`"));
    let end = text[start..]
        .find("\n}\n")
        .map(|offset| start + offset + 3)
        .unwrap_or_else(|| panic!("`{CONSTRUCTOR}` has a closing brace in the first column"));
    start..end
}

#[test]
fn every_lexer_and_parser_is_built_in_the_one_constructor() {
    let root = root();
    let parser_file = root.join(PARSER_FILE);
    let parser_text = std::fs::read_to_string(&parser_file)
        .unwrap_or_else(|error| panic!("read {PARSER_FILE}: {error}"));
    let constructor = constructor_range(&parser_text);

    let mut read = 0;
    let mut outside = Vec::new();
    let mut inside = 0;
    for directory in ["compiler/src", "cli/src"] {
        for path in rust_sources(&root.join(directory)) {
            read += 1;
            let relative = path
                .strip_prefix(&root)
                .expect("every source is under the root")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            for (offset, line) in sites(&text) {
                let is_lexer = text[offset..].starts_with("Lexer::new(");
                let in_constructor = relative == PARSER_FILE && constructor.contains(&offset);
                if relative != PARSER_FILE || (is_lexer && !in_constructor) {
                    outside.push(format!("{relative}:{line}"));
                } else {
                    inside += 1;
                }
            }
        }
    }

    assert!(read > 30, "the reader found {read} sources; it is wrong");
    // `parse.rs` holds two constructions: the one `Lexer::new` of the
    // constructor, and the `Parser::new_from` that consumes its lexer.
    assert!(
        inside >= 2,
        "the reader found {inside} constructions in {PARSER_FILE}; it is wrong"
    );
    assert!(
        outside.is_empty(),
        "a lexer or a parser outside `{CONSTRUCTOR}` of {PARSER_FILE} parses \
         without the §109.2 rule 5 byte check: {outside:?}"
    );
}

/// The firing control: the same reader, over a text that builds a lexer
/// outside the constructor, reports that site.
#[test]
fn the_reader_reports_a_construction_outside_the_constructor() {
    let text = concat!(
        "fn lexer_for() {\n",
        "    let one = Lexer::new(a, b, c, d);\n",
        "    let two = Lexer::new(a, b, c, d);\n",
        "}\n",
        "fn elsewhere() {\n",
        "    let three = Lexer::new(a, b, c, d);\n",
        "    let parser = Parser::new_from(three);\n",
        "}\n",
    );
    let constructor = constructor_range(text);
    let found = sites(text);
    assert_eq!(found.len(), 4, "{found:?}");
    let outside: Vec<usize> = found
        .iter()
        .filter(|(offset, _)| !constructor.contains(offset))
        .map(|(_, line)| *line)
        .collect();
    assert_eq!(outside, [6, 7], "the reader must report the two late sites");
}
