//! compiler.md §195.5 rule 2: each nullable builtin result row agrees with
//! the declared result in the TypeScript lib or the prelude.

use super::NULLABLE_RESULTS;
use crate::types::Absence;
use std::path::{Path, PathBuf};

/// The declaration files that a row can name: each TypeScript lib file and
/// the prelude.
fn declaration_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("node_modules/typescript/lib"))
        .expect("read the TypeScript lib directory")
        .map(|entry| entry.expect("read a lib entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("lib.") && name.ends_with(".d.ts"))
        })
        .collect();
    files.sort();
    files.push(root.join("prelude/lang.d.ts"));
    files
}

/// The declared result text of each signature of `member` inside each
/// `interface` or `class` named `declaration` in `text`.
fn declared_results(text: &str, declaration: &str, member: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        let opens = ["interface ", "class "].iter().any(|keyword| {
            trimmed.split_once(keyword).is_some_and(|(prefix, rest)| {
                (prefix.is_empty() || prefix.trim_end() == "declare")
                    && rest
                        .strip_prefix(declaration)
                        .is_some_and(|tail| tail.starts_with('<') || tail.starts_with(' '))
            })
        });
        if opens {
            inside = true;
            continue;
        }
        if inside && trimmed == "}" {
            inside = false;
            continue;
        }
        let signature = trimmed
            .strip_prefix(member)
            .filter(|tail| tail.starts_with('(') || tail.starts_with('<'));
        if let (true, Some(signature)) = (inside, signature) {
            let code = signature.split("//").next().unwrap_or_default();
            if let Some((_, result)) = code.rsplit_once("): ") {
                results.push(result.trim_end().trim_end_matches(';').to_owned());
            }
        }
    }
    results
}

fn spelling(result: &str) -> Option<Absence> {
    let members: Vec<&str> = result.split('|').map(str::trim).collect();
    match (members.contains(&"null"), members.contains(&"undefined")) {
        (true, false) => Some(Absence::Null),
        (false, true) => Some(Absence::Undefined),
        _ => None,
    }
}

#[test]
fn each_nullable_result_row_agrees_with_its_declaration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let files: Vec<(PathBuf, String)> = declaration_files(root)
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("read a declaration file");
            (path, text)
        })
        .collect();
    let mut failures = Vec::new();
    for row in NULLABLE_RESULTS {
        let declared: Vec<(String, String)> = files
            .iter()
            .flat_map(|(path, text)| {
                declared_results(text, row.declaration, row.member)
                    .into_iter()
                    .map(|result| (path.display().to_string(), result))
            })
            .collect();
        if declared.is_empty() {
            failures.push(format!(
                "{}.{}: no declaration found",
                row.declaration, row.member
            ));
        }
        for (path, result) in declared {
            if spelling(&result) != Some(row.absence) {
                failures.push(format!(
                    "{}.{}: the table has {:?}; {path} declares `{result}`",
                    row.declaration, row.member, row.absence
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Control: a row whose spelling differs from its declaration is reported.
#[test]
fn a_row_against_its_declaration_fires() {
    let text = "interface Map<K, V> {\n    get(key: K): V | undefined;\n}\n";
    let declared = declared_results(text, "Map", "get");
    assert_eq!(declared, vec!["V | undefined".to_owned()]);
    assert_eq!(spelling(&declared[0]), Some(Absence::Undefined));
    assert_ne!(spelling(&declared[0]), Some(Absence::Null));
    let prelude = "declare class Worker<In extends object, Out extends object> {\n  poll(): Out | null;  // never blocks\n}\n";
    assert_eq!(
        declared_results(prelude, "Worker", "poll"),
        vec!["Out | null".to_owned()]
    );
}
