//! A generic constructor and a generic method infer their type arguments (compiler.md §197).

use subscript_compiler::{check_program, divergence::Divergence, Diagnostic, RuleCode, SourceFile};

const BOX: &str = "class Box<T> { v: T; constructor(v: T) { this.v = v; } }\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    match check_program(&[SourceFile::new("test.ts", source)]) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics,
    }
}

fn accept(source: &str) {
    let diagnostics = diagnostics(source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn reject(source: &str, code: RuleCode, message: &str, divergence: Option<Divergence>) {
    let diagnostics = diagnostics(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, code, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, message);
    assert_eq!(diagnostics[0].divergence, divergence);
}

fn main_with(statements: &str) -> String {
    format!("{BOX}export function main(): void {{ {statements} }}\n")
}

/// §197 rule 3: a non-literal argument, then the contextual type, then the C4 default.
#[test]
fn contextual_candidate_follows_arguments_and_precedes_literal_defaults() {
    // A non-literal argument decides, so a different context is a type error.
    accept(&main_with(
        "const n: i64 = 1; const b: Box<i64> = new Box(n);",
    ));
    reject(
        &main_with("const n: i32 = 1; const b: Box<i64> = new Box(n);"),
        RuleCode::S005,
        "nominal types are not interchangeable: the initializer expects `Box<i64>`, got `Box<i32>`",
        Some(Divergence::ErasedAssignableTypeMismatch),
    );
    // A literal argument takes the contextual type: the value does not fit `i32`.
    accept(&main_with("const b: Box<i64> = new Box(3000000000);"));
    accept(&main_with(
        "const b: Box<i64> | null = new Box(3000000000);",
    ));
    // With no context, the C4 default decides.
    accept(&main_with("const c = new Box(7); const d: Box<i32> = c;"));
    reject(
        &main_with("const c = new Box(7); const e: Box<i64> = c;"),
        RuleCode::S005,
        "nominal types are not interchangeable: the initializer expects `Box<i64>`, got `Box<i32>`",
        Some(Divergence::ErasedAssignableTypeMismatch),
    );
}

#[test]
fn every_contextual_position_gives_a_candidate() {
    accept(&format!(
        "{BOX}class Holder {{ b: Box<i64> = new Box(1); }}\n\
         function take(b: Box<i64>): i64 {{ return b.v; }}\n\
         function make(): Box<i64> {{ return new Box(2); }}\n\
         export function main(): void {{\n\
         const xs: Box<i64>[] = [new Box(3)];\n\
         take(new Box(4)); make(); new Holder(); print(`${{xs.length}}`);\n\
         }}\n"
    ));
}

#[test]
fn constructor_candidates_come_from_its_parameters() {
    accept(
        "class P<T> { v: T; constructor(v: T, n: i32 = 0) { this.v = v; } }\n\
         class Item { id: i32 = 0; }\n\
         export function main(): void { const p = new P(new Item()); const q: P<Item> = p; }\n",
    );
    accept(
        "class Bag<T> { items: T[] = []; }\n\
         export function main(): void { const b: Bag<string> = new Bag(); }\n",
    );
    reject(
        "class Bag<T> { items: T[] = []; }\n\
         export function main(): void { const b = new Bag(); }\n",
        RuleCode::S100,
        "cannot infer type parameter `T` of `Bag`: no candidate; use explicit type arguments",
        Some(Divergence::GenericInferenceMissing),
    );
    reject(
        "class Two<T> { constructor(a: T, b: T) {} }\n\
         export function main(): void { const n: i32 = 1; const f: f64 = 2.5; new Two(n, f); }\n",
        RuleCode::S100,
        "cannot infer type parameter `T` of `Two`: conflicting candidates `i32` and `f64`; use explicit type arguments",
        Some(Divergence::GenericInferenceCandidates),
    );
}

#[test]
fn an_inferred_argument_reports_once() {
    let diagnostics = diagnostics(&main_with("new Box(missing);"));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S016, "{diagnostics:?}");
}

#[test]
fn generic_methods_infer_from_arguments_only() {
    let class = "class K {\n\
                 static make<T>(x: T): T[] { return [x]; }\n\
                 pick<T>(a: T, b: T): T { return a; }\n\
                 none<T>(): T[] { return []; }\n\
                 }\n";
    accept(&format!(
        "{class}export function main(): void {{\n\
         const n: i64 = 1; const k = new K();\n\
         const a: i64 = k.pick(n, 2); const b: i64[] = K.make(n);\n\
         }}\n"
    ));
    reject(
        &format!(
            "{class}export function main(): void {{ const k = new K(); const f: f64 = 1.5; const n: i32 = 1; k.pick(n, f); }}\n"
        ),
        RuleCode::S100,
        "cannot infer type parameter `T` of `K.pick`: conflicting candidates `i32` and `f64`; use explicit type arguments",
        Some(Divergence::GenericInferenceCandidates),
    );
    // The result context gives no candidate for a method.
    reject(
        &format!("{class}export function main(): void {{ const k = new K(); const xs: i32[] = k.none(); }}\n"),
        RuleCode::S100,
        "cannot infer type parameter `T` of `K.none`: no candidate; use explicit type arguments",
        Some(Divergence::GenericInferenceMissing),
    );
}

#[test]
fn map_and_set_take_their_type_arguments_from_context_or_source() {
    accept(
        "class Item { id: i32 = 0; }\n\
         class R { m: Map<string, Item> = new Map(); s: Set<string> = new Set(); }\n\
         export function main(): void {\n\
         const m: Map<string, Item> | null = new Map();\n\
         const xs: string[] = [\"a\"]; const s = new Set(xs); const t: Set<string> = s;\n\
         const u: Set<i64> = new Set([3000000000]);\n\
         const v = new Set([1, 2]); const w: Set<i32> = v; new R();\n\
         }\n",
    );
    reject(
        "export function main(): void { const m = new Map(); }\n",
        RuleCode::S100,
        "`new Map` requires explicit type arguments (Q24)",
        Some(Divergence::GenericConstructorTypeArguments),
    );
    reject(
        "export function main(): void { const s = new Set(); }\n",
        RuleCode::S100,
        "`new Set` requires explicit type arguments (Q24)",
        Some(Divergence::GenericConstructorTypeArguments),
    );
    reject(
        "export function main(): void { const x: i32 = new Map(); }\n",
        RuleCode::S100,
        "cannot infer type parameter `K` of `Map`: no candidate; use explicit type arguments",
        Some(Divergence::GenericInferenceMissing),
    );
}
