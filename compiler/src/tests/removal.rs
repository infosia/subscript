//! compiler.md §196.1: `pop()` and `shift()` give `T | undefined` for a
//! `T` in the `Map.get` set, and `T` for each other element type.

use super::*;

const DECLARATIONS: &str = "class Job { id: i32 = 1; }\n@ValueType\nclass Point { x: i32 = 0; }\n";

/// Checks `const r: T = xs.<member>();` with `T` the element type.
fn binds_as_element(element: &str, member: &str) -> bool {
    let source = format!(
        "{DECLARATIONS}export function main(): void {{\n  const xs: ({element})[] = [];\n  const r: {element} = xs.{member}();\n}}\n"
    );
    check_one(&source).is_ok()
}

/// Checks a test for `undefined` on the result before its use.
fn narrows_from_undefined(element: &str, member: &str) -> bool {
    let source = format!(
        "{DECLARATIONS}export function main(): void {{\n  const xs: ({element})[] = [];\n  const r = xs.{member}();\n  if (r !== undefined) {{ const s: {element} = r; }}\n}}\n"
    );
    check_one(&source).is_ok()
}

/// §196.1 rule 1 reads the `T | null` rule of §123.1. This check derives
/// that fact from the declaration of a nullable parameter, so it shares
/// nothing with the result of `pop`.
fn has_a_null_form(element: &str) -> bool {
    let source = format!(
        "{DECLARATIONS}function f(x: {element} | null): void {{}}\nexport function main(): void {{}}\n"
    );
    !element.ends_with("| null") && check_one(&source).is_ok()
}

#[test]
fn pop_and_shift_give_undefined_only_for_the_map_get_set() {
    let elements = [
        "Job",
        "string",
        "i32[]",
        "Map<i32, i32>",
        "Set<i32>",
        "(() => i32)",
        "i32",
        "Point",
        "Promise<i32>",
        "Job | null",
        "Promise<i32>[]",
    ];
    let set: Vec<&str> = elements
        .into_iter()
        .filter(|element| has_a_null_form(element))
        .collect();
    assert!(set.contains(&"string") && set.contains(&"Job"), "{set:?}");
    assert!(!set.contains(&"i32") && !set.contains(&"Point"), "{set:?}");
    for member in ["pop", "shift"] {
        for element in elements {
            let absent = set.contains(&element);
            assert_eq!(
                binds_as_element(element, member),
                !absent,
                "{element}[].{member}()"
            );
            if absent {
                assert!(
                    narrows_from_undefined(element, member),
                    "{element}[].{member}() must narrow from `undefined`"
                );
            }
        }
    }
}

/// §219.1 rule 3: a read of the result needs a test, as `tsc` requires.
#[test]
fn a_string_removal_needs_a_test_before_a_read() {
    for member in ["pop", "shift"] {
        let source = |body: &str| {
            format!("export function main(): void {{\n  const xs: string[] = [];\n  {body}\n}}\n")
        };
        for rejected in [
            format!("const n: i32 = xs.{member}().length;"),
            format!("const r = xs.{member}(); const n: i32 = r.length;"),
            format!("const s: string = xs.{member}();"),
        ] {
            assert!(check_one(&source(&rejected)).is_err(), "{rejected}");
        }
        for accepted in [
            format!("xs.{member}();"),
            format!("const r = xs.{member}(); if (r !== undefined) {{ const n: i32 = r.length; }}"),
            format!("const s: string = xs.{member}() ?? \"x\";"),
        ] {
            assert!(check_one(&source(&accepted)).is_ok(), "{accepted}");
        }
    }
}
