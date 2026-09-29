//! compiler.md §130: the former value-class decorator remains only in the record.
//!
//! Cost: one `git ls-files` call and one read of every listed file (about
//! 1,800 files); the two scans below share that one read.

use std::fs;
use std::path::Path;
use std::process::Command;

/// The former spelling, lower case. The match ignores ASCII case. The
/// name is built from two parts, so this file holds no hit of its own.
const FORMER: &str = concat!("c", "struct");

/// Paths that keep the former spelling as the record.
fn is_record(path: &str) -> bool {
    path.starts_with("specs/tracking/")
        || path == "specs/blocks/compiler-history.md"
        || path == "specs/blocks/compiler/s130-the-value-class-decorator-is-valuetype.md"
}

fn contains_former(text: &str) -> bool {
    text.to_ascii_lowercase().contains(FORMER)
}

/// Every hit as `path` (a hit in the path) or `path:line`. With
/// `exceptions` false, the record is a hit too.
fn forbidden_hits(path: &str, contents: &[u8], exceptions: bool) -> Vec<String> {
    if exceptions && is_record(path) {
        return Vec::new();
    }
    let mut hits = Vec::new();
    if contains_former(path) {
        hits.push(path.to_string());
    }
    for (index, raw) in contents.split(|byte| *byte == b'\n').enumerate() {
        let line = String::from_utf8_lossy(raw);
        if contains_former(&line) {
            hits.push(format!("{path}:{}", index + 1));
        }
    }
    hits
}

struct TrackedFiles {
    files: Vec<(String, Vec<u8>)>,
    non_utf8: Vec<String>,
}

/// Reads every file that git tracks or that `.gitignore` does not exclude.
/// A listed path that is absent from the working tree is a staged deletion.
fn tracked_files() -> TrackedFiles {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(&root)
        .output()
        .expect("list repository files");
    assert!(output.status.success(), "git ls-files failed");

    let mut tracked = TrackedFiles {
        files: Vec::new(),
        non_utf8: Vec::new(),
    };
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let path = std::str::from_utf8(raw)
            .expect("UTF-8 repository path")
            .to_string();
        let file = root.join(&path);
        if !file.exists() {
            continue;
        }
        let contents = fs::read(&file).unwrap_or_else(|error| panic!("read {path}: {error}"));
        if std::str::from_utf8(&contents).is_err() {
            tracked.non_utf8.push(path.clone());
        }
        tracked.files.push((path, contents));
    }
    tracked
}

#[test]
fn the_scan_detects_the_former_spelling_in_any_case() {
    let mixed = format!("ok\n@c{}\n", "Struct");
    assert_eq!(
        forbidden_hits("compiler/src/hir.rs", mixed.as_bytes(), true),
        vec!["compiler/src/hir.rs:2"]
    );
    let upper_path = format!("docs/C{}.md", "STRUCT");
    assert_eq!(forbidden_hits(&upper_path, b"", true), vec![upper_path]);
    assert_eq!(
        forbidden_hits("compiler/src/hir.rs", &[0xff, b'\n', b'C', b'S'], true),
        Vec::<String>::new()
    );
    assert_eq!(
        forbidden_hits(
            "compiler/src/hir.rs",
            &[0xff, b'C', b'S', b't', b'r', b'u', b'c', b't'],
            true
        ),
        vec!["compiler/src/hir.rs:1"]
    );
    let record = format!("C{}", "Struct");
    assert!(forbidden_hits("specs/tracking/x.md", record.as_bytes(), true).is_empty());
    assert_eq!(
        forbidden_hits("specs/tracking/x.md", record.as_bytes(), false),
        vec!["specs/tracking/x.md:1"]
    );
}

#[test]
fn former_decorator_spelling_is_limited_to_the_record() {
    let tracked = tracked_files();
    assert!(!tracked.files.is_empty(), "git ls-files listed no file");
    let prelude = tracked
        .files
        .iter()
        .find(|(path, _)| path == "prelude/lang.d.ts")
        .expect("the scan reads prelude/lang.d.ts");
    assert!(
        String::from_utf8_lossy(&prelude.1).contains("ValueType"),
        "prelude/lang.d.ts declares ValueType"
    );
    if !tracked.non_utf8.is_empty() {
        eprintln!(
            "scanned as bytes, not UTF-8: {}",
            tracked.non_utf8.join(", ")
        );
    }

    // Firing control: without the record exclusion, the same enumeration
    // finds a hit in each record path. If the enumeration or the match
    // loses a directory, a file, or a case, this fails.
    let unexcepted: Vec<String> = tracked
        .files
        .iter()
        .flat_map(|(path, contents)| forbidden_hits(path, contents, false))
        .collect();
    for expected in [
        "specs/tracking/",
        "specs/blocks/compiler-history.md:",
        "specs/blocks/compiler/s130-the-value-class-decorator-is-valuetype.md:",
    ] {
        assert!(
            unexcepted.iter().any(|hit| hit.starts_with(expected)),
            "the unexcepted scan finds no hit in {expected}"
        );
    }

    let hits: Vec<String> = tracked
        .files
        .iter()
        .flat_map(|(path, contents)| forbidden_hits(path, contents, true))
        .collect();
    assert!(
        hits.is_empty(),
        "old decorator spelling in {} scanned files: {}",
        tracked.files.len(),
        hits.join(", ")
    );
}
