//! compiler.md §235 rule 5d: each check of a value against a place uses
//! the check that takes the value, so that it can record a function
//! conversion. This total check lists each remaining call of the type-only
//! check `require_assignable` with its reason, and fails on a new call.
//!
//! Cost: one line scan of the checker sources, with no compilation.

use std::{collections::BTreeMap, fs, path::Path};

/// Each remaining type-only call, by file and enclosing function: the count
/// of calls and the reason that no value flows to a place there.
const ALLOWED: &[(&str, &str, usize, &str)] = &[
    (
        "stmt.rs",
        "check_for_of",
        2,
        "The binding annotation and the element type are compared in both directions, so they are the same type and need no conversion.",
    ),
    (
        "expr/call.rs",
        "check_set_source",
        1,
        "The element type of a `Set` source is a type of a collection, not a value; the strict check accepts no conversion.",
    ),
    (
        "expr/method.rs",
        "check_array_from",
        1,
        "The element type of an `Array.from` source is a type of a collection, not a value.",
    ),
    (
        "expr/method.rs",
        "check_set_method",
        1,
        "The key type of a `Set` argument is a type of a collection, not a value.",
    ),
    (
        "expr/aggregate.rs",
        "check_array_spread_lit",
        1,
        "The element type of a spread source is a type of a collection, not a value.",
    ),
    (
        "expr/assign.rs",
        "check_assign",
        1,
        "A compound assignment result is a number or a string, never a function value.",
    ),
];

/// Each call of the type-only check, by file and enclosing function.
fn calls(root: &Path) -> BTreeMap<(String, String), usize> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read the checker directory") {
            let path = entry.expect("read an entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let file = path
                .strip_prefix(root)
                .expect("a checker file")
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path).expect("read a checker file");
            let mut function = String::new();
            for line in source.lines() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                if let Some(rest) = code.split("fn ").nth(1).filter(|_| {
                    code.starts_with("fn ") || code.starts_with("pub") && code.contains(" fn ")
                }) {
                    function = rest
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .next()
                        .unwrap_or_default()
                        .to_string();
                }
                if function == "require_assignable" && code.contains("fn require_assignable") {
                    continue;
                }
                let count = code.matches(".require_assignable(").count();
                if count > 0 {
                    *found.entry((file.clone(), function.clone())).or_insert(0) += count;
                }
            }
        }
    }
    found
}

fn violations(found: &BTreeMap<(String, String), usize>) -> Vec<String> {
    let allowed: BTreeMap<(String, String), usize> = ALLOWED
        .iter()
        .map(|(file, function, count, reason)| {
            assert!(!reason.is_empty());
            ((file.to_string(), function.to_string()), *count)
        })
        .collect();
    let mut problems = Vec::new();
    for (key, count) in found {
        match allowed.get(key) {
            Some(expected) if expected == count => {}
            Some(expected) => problems.push(format!(
                "{}::{}: {count} type-only call(s), {expected} listed",
                key.0, key.1
            )),
            None => problems.push(format!(
                "{}::{}: {count} unlisted type-only call(s); use require_expr_assignable",
                key.0, key.1
            )),
        }
    }
    for key in allowed.keys() {
        if !found.contains_key(key) {
            problems.push(format!("stale entry: {}::{}", key.0, key.1));
        }
    }
    problems
}

#[test]
fn each_type_only_assignability_call_is_listed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let problems = violations(&calls(&root));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The firing control: a new unlisted call is a violation.
#[test]
fn an_unlisted_call_is_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let mut found = calls(&root);
    *found
        .entry(("expr/operator.rs".into(), "check_as".into()))
        .or_insert(0) += 1;
    let problems = violations(&found);
    assert_eq!(
        problems,
        ["expr/operator.rs::check_as: 1 unlisted type-only call(s); use require_expr_assignable"]
    );
}
