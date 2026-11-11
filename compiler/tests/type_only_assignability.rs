//! compiler.md §235 rule 5d and §240 rule 2a: each check of a value
//! against a place uses the check that takes the value, so that it can
//! record a function conversion. This total check lists each remaining call
//! of the type-only checks `require_assignable`, `assignable`,
//! `type_fits`, and `function_fits` with its reason, and fails on a new
//! call.
//!
//! Cost: one line scan of the checker sources (about 200 files) per name,
//! four in all, with no compilation.

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
        "The element type of a spread source is a type of a collection, not a value; each other element takes the check that takes the value.",
    ),
    (
        "expr/assign.rs",
        "check_assign",
        1,
        "A compound assignment result is a number or a string, never a function value.",
    ),
];

/// Each remaining `assignable` call, by file and enclosing function: the
/// count of calls and the reason that no conversion applies there.
const ALLOWED_ASSIGNABLE: &[(&str, &str, usize, &str)] = &[
    (
        "opaque.rs",
        "generic_overlap",
        4,
        "Two types are compared in both directions to decide an overlap; no value flows.",
    ),
    (
        "async_return.rs",
        "adopt_returned_handle",
        1,
        "The value type of a returned handle is compared with the result; a handle value is not a function value.",
    ),
    (
        "generics.rs",
        "satisfies_constraint_through_parameters",
        2,
        "A type argument is compared with a constraint; no value flows.",
    ),
    (
        "rejection_facts.rs",
        "ts_erased_assignable_inner",
        1,
        "A diagnostic classification compares two types after the check failed; no value flows.",
    ),
    (
        "type_rules.rs",
        "needs_function_conversion",
        1,
        "This is the decision of the conversion itself.",
    ),
    (
        "type_rules.rs",
        "adapt_function_reference",
        1,
        "The type of the adapter of compiler.md §246 rule 1 is checked against the place before the adapter is built; the value then takes the check that takes the value.",
    ),
    (
        "type_rules.rs",
        "dropped_handle",
        1,
        "The diagnostic of compiler.md §247 rule 1 decides with the adapted type that the adapter would build; no value is placed.",
    ),
    (
        "type_rules.rs",
        "needs_adapter_name",
        1,
        "The diagnostic of compiler.md §246 rule 5 decides with the adapted type that rule 1 would build; no value is placed.",
    ),
    (
        "type_rules.rs",
        "place_value",
        1,
        "This is the check that takes the value: it reads the type after the conversion.",
    ),
    (
        "type_rules.rs",
        "converts_across_spellings",
        1,
        "Two nullable spellings of one type are compared; no function conversion applies to an absence spelling.",
    ),
    (
        "type_rules.rs",
        "require_assignable",
        1,
        "This is the type-only check for a type where no value flows; it rejects each type that needs a conversion.",
    ),
    (
        "expr/member.rs",
        "check_index",
        2,
        "An index is compared with `i32`, which is not a function type.",
    ),
    (
        "expr/callback_shape.rs",
        "callback_type_fits",
        1,
        "A parameter type of a callback that involves a type parameter is compared; the callback value takes its place in `expect_callback_shape`.",
    ),
    (
        "expr/type_arguments.rs",
        "reject_widened_reduce_result",
        1,
        "`string` is compared with an accumulator type to choose a diagnostic; no value flows.",
    ),
];

/// Each remaining `type_fits` or `function_fits` call, by file and
/// enclosing function: the count of calls and the reason that no
/// conversion applies there.
const ALLOWED_FITS: &[(&str, &str, usize, &str)] = &[
    (
        "expr/callback_shape.rs",
        "callback_type_fits",
        1,
        "A parameter or result type of a built-in callback is compared with its place; the callback value took the conversion of §240 rule 2 before, and `place_callback_value` converts it after.",
    ),
    (
        "type_rules.rs",
        "assignable_through_constraints",
        1,
        "This is the rule of `assignable` for two function types; it compares types, and a value check calls it after the conversion.",
    ),
    (
        "type_rules.rs",
        "prefix_parameters_fit",
        1,
        "This is the parameter half of the decision of the conversions of §240 rule 2 and §243 rules 1 and 2 itself.",
    ),
    (
        "type_rules.rs",
        "result_fits",
        1,
        "This is the result half of the decision of the conversions of §240 rule 2 and §243 rules 1 and 2 itself.",
    ),
    (
        "type_rules.rs",
        "discard_lambda_result",
        1,
        "This is the decision of the conversion of §242 rule 2 itself.",
    ),
    (
        "type_rules.rs",
        "function_fits",
        2,
        "The recursion of §235 rule 3 over parameter and result types; no value flows.",
    ),
    (
        "type_rules.rs",
        "needs_function_conversion",
        1,
        "This is the decision of the conversion of §235 rule 5a itself.",
    ),
    (
        "type_rules.rs",
        "type_fits",
        2,
        "The recursion of §235 rule 2 over nullable and function types; no value flows.",
    ),
];

/// Each call of the type-only check `require_assignable`, by file and
/// enclosing function.
fn calls(root: &Path) -> BTreeMap<(String, String), usize> {
    calls_of(root, "require_assignable")
}

/// Each call of the type-only check `name`, by file and enclosing
/// function. The definition of `name` is not a call.
fn calls_of(root: &Path, name: &str) -> BTreeMap<(String, String), usize> {
    let needle = format!(".{name}(");
    let definition = format!("fn {name}");
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
                if function == name && code.contains(&definition) {
                    continue;
                }
                let count = code.matches(needle.as_str()).count();
                if count > 0 {
                    *found.entry((file.clone(), function.clone())).or_insert(0) += count;
                }
            }
        }
    }
    found
}

fn violations(found: &BTreeMap<(String, String), usize>) -> Vec<String> {
    violations_of(found, ALLOWED)
}

fn violations_of(
    found: &BTreeMap<(String, String), usize>,
    listed: &[(&str, &str, usize, &str)],
) -> Vec<String> {
    let allowed: BTreeMap<(String, String), usize> = listed
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

#[test]
fn each_type_only_assignable_call_is_listed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let problems = violations_of(&calls_of(&root, "assignable"), ALLOWED_ASSIGNABLE);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The calls of `type_fits` and `function_fits`, by file and enclosing
/// function.
fn fits_calls(root: &Path) -> BTreeMap<(String, String), usize> {
    let mut found = calls_of(root, "type_fits");
    for (key, count) in calls_of(root, "function_fits") {
        *found.entry(key).or_insert(0) += count;
    }
    found
}

#[test]
fn each_type_only_fits_call_is_listed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let problems = violations_of(&fits_calls(&root), ALLOWED_FITS);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The firing control of the `type_fits` and `function_fits` scan: a new
/// unlisted call is a violation.
#[test]
fn an_unlisted_fits_call_is_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let mut found = fits_calls(&root);
    *found
        .entry(("expr/aggregate.rs".into(), "check_array_spread_lit".into()))
        .or_insert(0) += 1;
    let problems = violations_of(&found, ALLOWED_FITS);
    assert_eq!(
        problems,
        ["expr/aggregate.rs::check_array_spread_lit: 1 unlisted type-only call(s); use require_expr_assignable"]
    );
}

/// The firing control of the `assignable` scan: a new unlisted call is a
/// violation.
#[test]
fn an_unlisted_assignable_call_is_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let mut found = calls_of(&root, "assignable");
    *found
        .entry((
            "expr/logical_assign.rs".into(),
            "nullish_assignment_type".into(),
        ))
        .or_insert(0) += 1;
    let problems = violations_of(&found, ALLOWED_ASSIGNABLE);
    assert_eq!(
        problems,
        ["expr/logical_assign.rs::nullish_assignment_type: 1 unlisted type-only call(s); use require_expr_assignable"]
    );
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
