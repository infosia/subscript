//! An assignment to a method names the method (compiler.md §209).
//!
//! Cost: 9 checker calls on one-file programs, with no code generation
//! and no external process.

use super::*;
use crate::divergence::Divergence;

fn check(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::entry("main.ts", src)])
}

/// The first diagnostic of `src`, which the checker rejects.
fn first(src: &str) -> Diagnostic {
    check(src)
        .expect_err(src)
        .into_iter()
        .next()
        .expect("a rejected program has a diagnostic")
}

/// Asserts the S100 text and the divergence of the first diagnostic.
/// `divergence` is `None` for a form that `tsc` rejects (§209.1 rule 3).
fn assert_rejected(src: &str, message: &str, divergence: Option<Divergence>) {
    let diagnostic = first(src);
    assert_eq!(diagnostic.code, RuleCode::S100, "{src}");
    assert_eq!(diagnostic.message, message, "{src}");
    assert_eq!(diagnostic.divergence, divergence, "{src}");
}

const CLASS: &str = "class C { v(): i32 { return 1; } }\n\
                     function two(): i32 { return 2; }\n";

/// Rule 1: a target that is an instance method of a class receiver.
#[test]
fn an_instance_method_target_names_the_class() {
    let message = "method `v` of `C` cannot be assigned: a method is a function, not a field";
    assert_rejected(
        &format!("{CLASS}export function main(): void {{ const c = new C(); c.v = two; }}\n"),
        message,
        Some(Divergence::MethodAssignment),
    );
    assert_rejected(
        &format!("{CLASS}export function main(): void {{ const c = new C(); c.v += 1; }}\n"),
        message,
        None,
    );
}

/// Rule 1: a target on a receiver of type `T extends I` names `I`.
#[test]
fn a_constrained_method_target_names_the_interface() {
    let interface = "interface I { v(): i32; }\nfunction two(): i32 { return 2; }\n";
    let message = "method `v` of `I` cannot be assigned: a method is a function, not a field";
    assert_rejected(
        &format!(
            "{interface}function f<T extends I>(t: T): void {{ t.v = two; }}\n\
             export function main(): void {{}}\n"
        ),
        message,
        Some(Divergence::MethodAssignment),
    );
    assert_rejected(
        &format!(
            "{interface}function f<T extends I>(t: T): void {{ t.v += 1; }}\n\
             export function main(): void {{}}\n"
        ),
        message,
        None,
    );
}

/// Rule 2: a target that is a static method.
#[test]
fn a_static_method_target_names_the_static_method() {
    let class = "class C { static s(): i32 { return 1; } }\n\
                 function two(): i32 { return 2; }\n";
    let message = "static method `C.s` cannot be assigned: a method is a function, not a field";
    assert_rejected(
        &format!("{class}export function main(): void {{ C.s = two; }}\n"),
        message,
        Some(Divergence::MethodAssignment),
    );
    assert_rejected(
        &format!("{class}export function main(): void {{ C.s += 1; }}\n"),
        message,
        None,
    );
}

/// Rule 3: an assignment to a `#x` method carries no block, because
/// `tsc` rejects it (TS2803).
#[test]
fn a_private_method_target_has_no_block() {
    assert_rejected(
        "class C { #v(): i32 { return 1; } w(): void { this.#v = two; } }\n\
         function two(): i32 { return 2; }\n\
         export function main(): void {}\n",
        "method `#v` of `C` cannot be assigned: a method is a function, not a field",
        None,
    );
    assert_rejected(
        "class C { static #s(): i32 { return 1; } static w(): void { C.#s = two; } }\n\
         function two(): i32 { return 2; }\n\
         export function main(): void {}\n",
        "static method `C.#s` cannot be assigned: a method is a function, not a field",
        None,
    );
}

/// Control: an assignment to a field of a function type is accepted.
#[test]
fn a_field_of_a_function_type_is_assigned() {
    check(
        "class C { v: () => i32 = (): i32 => 1; }\n\
         function two(): i32 { return 2; }\n\
         export function main(): void { const c = new C(); c.v = two; print(`${c.v()}`); }\n",
    )
    .unwrap();
}
