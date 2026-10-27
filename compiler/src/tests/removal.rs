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

#[test]
fn pop_and_shift_give_undefined_only_for_the_map_get_set() {
    let absent = ["Job", "i32[]", "Map<i32, i32>", "Set<i32>", "(() => i32)"];
    let kept = [
        "i32",
        "string",
        "Point",
        "Promise<i32>",
        "Job | null",
        "Promise<i32>[]",
    ];
    for member in ["pop", "shift"] {
        for element in absent {
            assert!(
                !binds_as_element(element, member),
                "{element}[].{member}() must give `{element} | undefined`"
            );
            assert!(
                narrows_from_undefined(element, member),
                "{element}[].{member}() must narrow from `undefined`"
            );
        }
        for element in kept {
            assert!(
                binds_as_element(element, member),
                "{element}[].{member}() must give `{element}`"
            );
        }
    }
}
