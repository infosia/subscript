//! A total check for retired reasons (compiler.md §104.4).
//!
//! A rejection states one reason. When a contract retires a reason, no
//! user-facing string keeps it. A fix that closes named sites does
//! not converge, so this test sweeps every string a user can see and
//! reports every remaining site at once.
//!
//! §104.4 asks each half of the sweep for its own non-empty guard. The
//! sweep has three parts, and each part carries a guard.
//!
//! The table part asks the compiler for the strings it builds from a
//! table. These are the divergence table, the rule and warning
//! explanations, and the generated reference. The rendered part checks
//! every reject-corpus entry and reads the diagnostic it renders. The
//! source part reads the string literals of every `.rs` file under the
//! shipped crates' `src` directories. A message that no corpus entry
//! reaches is covered there. This file holds the phrases as data and
//! lives outside those directories, so the sweep does not report
//! itself.

#[allow(dead_code)]
#[path = "../../codegen/tests/corpus/mod.rs"]
mod corpus;

use corpus::interop;

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{api_reference, check_program, render_diagnostics, RuleCode, WarnCode};

/// One retired reason: the phrase, and the record that retired it.
///
/// A phrase is lowercase; the sweep compares lowercase.
const RETIRED: &[(&str, &str)] = &[
    (
        "duplicate host entry",
        "compiler.md §129 retires the program-wide host name rule",
    ),
    (
        "was renamed to `@valuetype`",
        "compiler.md §130 rule 2: the former value-class decorator has no \
     rule of its own; the general unknown-decorator message rejects it",
    ),
    (
        "outlives its call",
        "compiler.md §103.3: `compiler.md` §70 landed a reference-counted \
     handle and a `Generator<T>` already outlives the call that made it, \
     so the obstacle is the missing view type (stdlib.md §14.3)",
    ),
];

/// The crates whose `.rs` sources build what a `subscript` user reads.
const SCANNED_CRATES: &[&str] = &["bindgen", "cli", "codegen", "compiler", "runtime"];

/// Answers an error when a part of the sweep read no string.
///
/// compiler.md §104.4: a part that reads nothing passes silently, which
/// is the firing-control defect this guard removes.
fn guard_non_empty(part: &str, read: usize) -> Result<(), String> {
    if read == 0 {
        return Err(format!(
            "the {part} part of the sweep read no string; the reader is wrong"
        ));
    }
    Ok(())
}

/// Answers an error when the rendered part read fewer diagnostics than
/// the reject corpus holds entries.
///
/// compiler.md §104.4: an entry that checks clean renders no
/// diagnostic, so the sweep never reads it. `entries` comes from the
/// corpus directory and `read` from the checker, so the two facts are
/// derived separately.
fn guard_every_entry_rendered(
    read: usize,
    entries: &[String],
    clean: &[String],
) -> Result<(), String> {
    if entries.is_empty() {
        return Err(
            "the rendered part found no reject corpus entry; the reader is wrong".to_owned(),
        );
    }
    if read != entries.len() || !clean.is_empty() {
        return Err(format!(
            "the rendered part read {read} diagnostic(s) for {} reject corpus entries. These entries checked clean: {}",
            entries.len(),
            if clean.is_empty() {
                "none".to_owned()
            } else {
                clean.join(", ")
            }
        ));
    }
    Ok(())
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the compiler crate has a parent directory")
        .to_path_buf()
}

/// Collects `(where, text)` for every string the compiler can build
/// from a table.
fn table_strings() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for divergence in Divergence::ALL {
        let entry = divergence.entry();
        out.push((
            format!("Divergence::{divergence:?}.ts"),
            entry.ts.to_owned(),
        ));
        out.push((
            format!("Divergence::{divergence:?}.subscript"),
            entry.subscript.to_owned(),
        ));
        out.push((
            format!("Divergence::{divergence:?}.why"),
            entry.why.to_owned(),
        ));
        out.push((
            format!("Divergence::{divergence:?}.collision"),
            entry.collision.to_owned(),
        ));
    }
    for code in RuleCode::ALL {
        out.push((
            format!("RuleCode::{}.explanation", code.as_str()),
            code.explanation().to_owned(),
        ));
    }
    for code in WarnCode::ALL {
        out.push((
            format!("WarnCode::{}.explanation", code.as_str()),
            code.explanation().to_owned(),
        ));
    }
    out.push((
        "api_reference::render_markdown".to_owned(),
        api_reference::render_markdown(),
    ));
    guard_non_empty("table", out.len()).unwrap_or_else(|error| panic!("{error}"));
    out
}

/// Renders the diagnostics of every reject-corpus entry. An assembled
/// message reaches a user through this path and through no table.
fn rendered_rejections(root: &Path) -> Vec<(String, String)> {
    let dir = root.join("corpus").join("reject");
    let mut out = Vec::new();
    let mut names = Vec::new();
    let mut clean = Vec::new();
    let mut selective_cost = std::time::Duration::ZERO;
    let mut mirrored_cost = std::time::Duration::ZERO;
    for name in corpus::entry_ids(&dir) {
        names.push(name.clone());
        let files = corpus::entry_sources(&dir, &name);
        let start = std::time::Instant::now();
        let checked = check_program(&files);
        selective_cost += start.elapsed();
        let Err(diagnostics) = checked else {
            clean.push(name);
            continue;
        };
        source_diagnostic_names(&diagnostics).unwrap();
        // A missing mirror produces S016 and can hide the intended rejection.
        // Only those entries need a comparison with the complete fixture set.
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::S016)
        {
            let mut mirrored = files.clone();
            for mirror in interop::mirrors(subscript_compiler::SourceFile::ambient) {
                if !mirrored.iter().any(|file| file.name == mirror.name) {
                    mirrored.insert(0, mirror);
                }
            }
            let start = std::time::Instant::now();
            let complete =
                check_program(&mirrored).expect_err("the reject entry requires an error");
            mirrored_cost += start.elapsed();
            source_diagnostic_names(&complete).unwrap();
            assert_eq!(
                (diagnostics[0].code, &diagnostics[0].message),
                (complete[0].code, &complete[0].message),
                "{name}: the selective reader must supply the mirrors for the intended rejection"
            );
        }
        out.push((name, render_diagnostics(&files, &diagnostics)));
    }
    eprintln!(
        "reject checks: {} entries, selective {selective_cost:?}, mirrored {mirrored_cost:?}",
        names.len()
    );
    guard_every_entry_rendered(out.len(), &names, &clean).unwrap_or_else(|error| panic!("{error}"));
    out
}

/// Collects `(file:line, literal)` for every string literal under the
/// shipped crates' `src` directories.
fn source_literals(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for crate_name in SCANNED_CRATES {
        let dir = root.join(crate_name).join("src");
        assert!(dir.is_dir(), "{} is not a directory", dir.display());
        collect_literals(&dir, root, &mut out);
    }
    guard_non_empty("source", out.len()).unwrap_or_else(|error| panic!("{error}"));
    out
}

fn collect_literals(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read
        .map(|entry| entry.expect("a source directory entry").path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_literals(&path, root, out);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let label = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        for (line, literal) in string_literals(&source) {
            out.push((format!("{label}:{line}"), literal));
        }
    }
}

/// Extracts the string literals of one Rust source, with the 1-based
/// line each one starts on. Comments, character literals, and lifetimes
/// are not literals and are skipped.
fn string_literals(source: &str) -> Vec<(u32, String)> {
    let bytes: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut index = 0usize;
    let mut line = 1u32;
    while index < bytes.len() {
        let c = bytes[index];
        if c == '\n' {
            line += 1;
            index += 1;
            continue;
        }
        if c == '/' && bytes.get(index + 1) == Some(&'/') {
            while index < bytes.len() && bytes[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if c == '/' && bytes.get(index + 1) == Some(&'*') {
            let mut depth = 1usize;
            index += 2;
            while index < bytes.len() && depth > 0 {
                if bytes[index] == '/' && bytes.get(index + 1) == Some(&'*') {
                    depth += 1;
                    index += 2;
                } else if bytes[index] == '*' && bytes.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    index += 2;
                } else {
                    if bytes[index] == '\n' {
                        line += 1;
                    }
                    index += 1;
                }
            }
            continue;
        }
        if c == '\'' {
            // A character literal, or a lifetime. Only the first is a
            // literal, and neither can hold a phrase.
            let escaped = bytes.get(index + 1) == Some(&'\\');
            let closed = bytes.get(index + 2) == Some(&'\'');
            if escaped || closed {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == '\\' {
                        index += 2;
                        continue;
                    }
                    if bytes[index] == '\'' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            } else {
                index += 1;
            }
            continue;
        }
        if let Some((start, hashes)) = raw_string_start(&bytes, index) {
            let (text, next, lines) = read_raw_string(&bytes, start, hashes);
            out.push((line, text));
            line += lines;
            index = next;
            continue;
        }
        if c == '"' {
            let (text, next, lines) = read_string(&bytes, index + 1);
            out.push((line, text));
            line += lines;
            index = next;
            continue;
        }
        index += 1;
    }
    out
}

/// Answers the index just past `r#*"` and the hash count, when a raw
/// string starts at `index`.
fn raw_string_start(bytes: &[char], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&'r') {
        return None;
    }
    if index > 0 {
        let previous = bytes[index - 1];
        if previous.is_alphanumeric() || previous == '_' {
            return None;
        }
    }
    cursor += 1;
    let mut hashes = 0usize;
    while bytes.get(cursor) == Some(&'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&'"') {
        Some((cursor + 1, hashes))
    } else {
        None
    }
}

fn read_raw_string(bytes: &[char], start: usize, hashes: usize) -> (String, usize, u32) {
    let mut text = String::new();
    let mut lines = 0u32;
    let mut index = start;
    while index < bytes.len() {
        if bytes[index] == '"' {
            let closes = (1..=hashes).all(|offset| bytes.get(index + offset) == Some(&'#'));
            if closes {
                return (text, index + 1 + hashes, lines);
            }
        }
        if bytes[index] == '\n' {
            lines += 1;
        }
        text.push(bytes[index]);
        index += 1;
    }
    (text, index, lines)
}

fn read_string(bytes: &[char], start: usize) -> (String, usize, u32) {
    let mut text = String::new();
    let mut lines = 0u32;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            '\\' => {
                // A line continuation joins the fragments of a wrapped
                // literal, so the phrase survives the wrap.
                if bytes.get(index + 1) == Some(&'\n') {
                    lines += 1;
                    index += 2;
                    while bytes.get(index).is_some_and(|c| *c == ' ' || *c == '\t') {
                        index += 1;
                    }
                    continue;
                }
                if let Some(escaped) = bytes.get(index + 1) {
                    text.push(*escaped);
                }
                index += 2;
            }
            '"' => return (text, index + 1, lines),
            other => {
                if other == '\n' {
                    lines += 1;
                }
                text.push(other);
                index += 1;
            }
        }
    }
    (text, index, lines)
}

#[test]
fn no_user_facing_string_states_a_retired_reason() {
    let root = repository_root();
    let mut sources = table_strings();
    sources.extend(rendered_rejections(&root));
    sources.extend(source_literals(&root));

    let mut violations = Vec::new();
    for (site, text) in &sources {
        let haystack = text.to_lowercase();
        for (phrase, record) in RETIRED {
            if haystack.contains(phrase) {
                violations.push(format!(
                    "{site}: states the retired reason {phrase:?} ({record})"
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} user-facing string(s) state a retired reason:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// The guards must be able to fail. Core principle 9: a test builds
/// the violating form.
#[test]
fn each_guard_reports_a_part_that_read_less() {
    assert!(guard_non_empty("table", 1).is_ok());
    let empty = guard_non_empty("table", 0).expect_err("an empty part must be reported");
    assert!(empty.contains("read no string"), "{empty}");

    let entries = ["r01.ts".to_owned(), "r02.ts".to_owned()];
    assert!(guard_every_entry_rendered(2, &entries, &[]).is_ok());
    let dropped = guard_every_entry_rendered(1, &entries, &["r02.ts".to_owned()])
        .expect_err("an entry that checks clean must be reported");
    assert!(dropped.contains("r02.ts"), "{dropped}");
    let none = guard_every_entry_rendered(0, &[], &[])
        .expect_err("an empty corpus directory must be reported");
    assert!(none.contains("no reject corpus entry"), "{none}");
}

/// The sweep must be able to fail. Core principle 9: a check needs a
/// control that fires.
#[test]
fn the_sweep_reads_a_literal_and_not_a_comment() {
    let source = concat!(
        "// a comment that names the phrase: it outlives its ",
        "call\n",
        "const A: &str = \"a held iterator outlives its \\\n    call\";\n",
        "const B: &str = r#\"another that outlives its call\"#;\n",
        "const C: char = '\"';\n",
        "fn f<'a>(x: &'a str) -> &'a str { x }\n",
    );
    let literals = string_literals(source);
    let hits: Vec<&(u32, String)> = literals
        .iter()
        .filter(|(_, text)| text.contains("outlives its call"))
        .collect();
    assert_eq!(
        hits.len(),
        2,
        "the reader must find both literals and neither comment: {literals:?}"
    );
    assert_eq!(hits[0].0, 2, "the wrapped literal starts on line 2");
    assert_eq!(hits[1].0, 4, "the raw literal starts on line 4");
    assert!(
        literals.iter().all(|(_, text)| !text.starts_with("// a")),
        "a comment is not a literal: {literals:?}"
    );
}

#[test]
fn interop_rejections_require_their_mirror() {
    let dir = repository_root().join("corpus/reject");
    for (name, code) in [
        ("r169-embedded-header-copy", RuleCode::S100),
        ("r251-map-boundary-struct-get", RuleCode::S014),
    ] {
        let files = corpus::entry_sources(&dir, name);
        let diagnostics = check_program(&files).expect_err("the mirrored entry is rejected");
        assert_eq!(diagnostics[0].code, code, "{name}: {diagnostics:?}");
        let source = fs::read_to_string(dir.join(format!("{name}.ts"))).unwrap();
        let bare = [subscript_compiler::SourceFile::new(
            format!("{name}.ts"),
            source,
        )];
        let missing =
            check_program(&bare).expect_err("the missing mirror must change the rejection");
        assert_eq!(missing[0].code, RuleCode::S016, "{name}: {missing:?}");
    }
}

fn source_diagnostic_names(diagnostics: &[subscript_compiler::Diagnostic]) -> Result<(), String> {
    let leaked: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("[[identity:") || d.message.contains("[[Error]]"))
        .collect();
    if leaked.is_empty() {
        Ok(())
    } else {
        Err(format!("internal symbols in diagnostics: {leaked:?}"))
    }
}

#[test]
fn diagnostic_name_check_detects_a_directly_built_internal_symbol() {
    use subscript_compiler::{Diagnostic, Pos};
    for name in ["[[identity:module:00]]C", "[[Error]]"] {
        let diagnostic = Diagnostic::new(
            RuleCode::S100,
            format!("got `{name}`"),
            Pos::new("main.ts", 1, 1),
        );
        assert!(source_diagnostic_names(&[diagnostic]).is_err());
    }
    let source = Diagnostic::new(
        RuleCode::S100,
        "got `C (main.ts)`",
        Pos::new("main.ts", 1, 1),
    );
    assert!(source_diagnostic_names(&[source]).is_ok());
}
