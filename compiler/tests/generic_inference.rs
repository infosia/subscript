use subscript_compiler::{check_program, RuleCode, SourceFile};

fn accept(source: &str) {
    check_program(&[SourceFile::new("test.ts", source)])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
}

fn reject(source: &str, message: &str) {
    let errors = check_program(&[SourceFile::new("test.ts", source)]).expect_err("must reject");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, message);
    assert_eq!(
        errors[0].divergence,
        Some(if message.contains("no candidate") {
            subscript_compiler::divergence::Divergence::GenericInferenceMissing
        } else {
            subscript_compiler::divergence::Divergence::GenericInferenceCandidates
        })
    );
}

#[test]
fn structural_candidates_cover_every_shape() {
    accept(
        r#"
class Box<T> { value: T; constructor(value: T) { this.value = value; } }
function fixed<T>(x: FixedArray<T, 2>): T { return x[0]; }
function set<T>(x: Set<T>): i32 { return x.size; }
function box<T>(x: Box<T>): T { return x.value; }
function apply<T, U>(f: (x: T) => U, x: T): U { return f(x); }
function convert(x: u8): string { return `${x}`; }
function id<T>(x: T): T { return x; }
function first<T>(x: T[]): T { return x[0]; }
export function main(): void {
    const xs: FixedArray<u8, 2> = [1, 2];
    const a: u8 = fixed(xs);
    const b: i32 = set(new Set<u8>());
    const c: u8 = box(new Box<u8>(1));
    const d: string = apply(convert, a);
    const e: i32 = id(-7);
    const f: f64 = id(2.5);
    const g: string = first(["text"]);
}
"#,
    );
}

#[test]
fn literals_follow_candidates_in_both_orders_and_arrays() {
    accept(
        r#"
function pair<T>(x: T, y: T): T { return x; }
export function main(): void {
    const n: i64 = 1;
    const a: i64 = pair(n, 3000000000);
    const b: i64 = pair(3000000000, n);
    const xs: u8[] = [1];
    const c: u8[] = pair(xs, [2]);
    const d: u8[] = pair([2], xs);
    const e: u8[] = pair([], xs);
}
"#,
    );
}

#[test]
fn nullable_candidates_join_in_both_orders() {
    accept(
        r#"
class Box { value: i32 = 0; }
function pair<T>(x: T, y: T): T { return x; }
function nullablePair<T>(x: T | null): T | null { return pair(x, null); }
export function main(): void {
    const b: Box | null = null;
    const a: Box | null = pair(b, new Box());
    const c: Box | null = pair(new Box(), b);
    const d: Box | null = pair(null, new Box());
    const e: Box | null = pair(new Box(), null);
    const f: Box | null = nullablePair<Box>(b);
}
"#,
    );
}

#[test]
fn inference_diagnostics_name_candidates_and_missing_parameters() {
    reject(r#"
function pair<T>(x: T, y: T): T { return x; }
export function main(): void { const a: i32 = 1; const b: f64 = 2.5; pair(a, b); }
"#, "cannot infer type parameter `T` of `pair`: conflicting candidates `i32` and `f64`; use explicit type arguments");
    reject(
        r#"
function empty<T>(x: T | null): T | null { return x; }
export function main(): void { empty(null); }
"#,
        "cannot infer type parameter `T` of `empty`: no candidate; use explicit type arguments",
    );
    reject(
        r#"
function empty<T>(): T | null { return null; }
export function main(): void { empty(); }
"#,
        "cannot infer type parameter `T` of `empty`: no candidate; use explicit type arguments",
    );
}

#[test]
fn constraints_and_opaque_body_checks_still_apply() {
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        r#"
class Box { value: i32 = 0; }
function constrained<T extends Box>(x: T): T { return x; }
export function main(): void { constrained(1); }
"#,
    )])
    .expect_err("the inferred i32 does not satisfy Box");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("constraint")),
        "{errors:?}"
    );
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        r#"
function bad<T>(x: T): boolean { return x > 1; }
export function main(): void { bad(1); }
"#,
    )])
    .expect_err("the opaque body rejects the operator");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("operator")),
        "{errors:?}"
    );
}

#[test]
fn inferred_requests_preserve_instance_chain_edges() {
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        r#"
class W<T> { value: T; constructor(value: T) { this.value = value; } }
function nest<T>(x: T, n: i32): i32 {
    if (n <= 0) { return 0; }
    return 1 + nest(new W<T>(x), n - 1);
}
export function main(): void { nest(1, 3); }
"#,
    )])
    .expect_err("the inferred arguments grow without bound");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, "generic template `nest`: the chain of instances grows without bound from `nest<T>` to `nest<W<T>>` through argument `W<T>`");
    accept(
        r#"
class W<T> { value: T; constructor(value: T) { this.value = value; } }
function finite<T>(x: T, n: i32): i32 {
    if (n <= 0) { return 0; }
    return 1 + finite(new W<i32>(1), n - 1);
}
export function main(): void { finite(1, 3); }
"#,
    );
    accept(
        r#"
function plain<T>(x: T, n: i32): T {
    if (n <= 0) { return x; }
    return plain(x, n - 1);
}
export function main(): void { plain(1, 3); }
"#,
    );
}

#[test]
fn fixed_parameters_keep_context_and_constraints_supply_shapes() {
    accept(
        r#"
@Descriptor class Options { count?: u8 = 1; }
function configured<T>(x: T, options: Options): T { return x; }
function first<T>(xs: T[]): T { return xs[0]; }
function wrapper<U extends u8[]>(xs: U): u8 { return first(xs); }
function fixed<T>(xs: FixedArray<T, 2>): T { return xs[0]; }
export function main(): void {
    const value: i32 = configured(7, { count: 2 });
    const xs: u8[] = [1];
    const element: u8 = wrapper(xs);
    const defaulted: i32 = fixed([1, 2]);
}
"#,
    );
}

#[test]
fn return_context_and_untyped_callbacks_do_not_supply_candidates() {
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        r#"
function id<T>(x: T): T { return x; }
export function main(): void { const value: i64 = id(7); }
"#,
    )])
    .expect_err("the return context cannot override the i32 default");
    assert!(
        errors.iter().any(|error| error.message.contains("i32")),
        "{errors:?}"
    );
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        r#"
function apply<T>(f: (x: T) => T, x: T): T { return f(x); }
export function main(): void { apply(x => x, 1); }
"#,
    )])
    .expect_err("a callback parameter needs a declared type");
    assert!(!errors.is_empty());
    reject(
        r#"
function first<T>(xs: T[]): T { return xs[0]; }
export function main(): void { first([]); }
"#,
        "cannot infer type parameter `T` of `first`: no candidate; use explicit type arguments",
    );
}

#[test]
fn constrained_parameters_preserve_instance_edge_identity() {
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        r#"
class Box { value: i32 = 0; }
class W<T> { value: T; constructor(value: T) { this.value = value; } }
function nest<T extends Box>(x: T, n: i32): i32 {
    if (n <= 0) { return 0; }
    return 1 + nest(new W<T>(x), n - 1);
}
export function main(): void { nest(new Box(), 3); }
"#,
    )])
    .expect_err("the constraint cannot remove a growing edge");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].message.contains("grows without bound"),
        "{errors:?}"
    );
}

#[test]
fn literal_array_errors_report_once_at_each_call_site() {
    let files = [
        SourceFile::new("lib.ts", "export function id<T>(x: T): T { return x; }"),
        SourceFile::entry(
            "main.ts",
            r#"
import * as ns from "./lib";
function first<T>(xs: T[]): T { return xs[0]; }
function second<T, U>(x: T, y: U): U { return y; }
async function af<T>(xs: T[]): Promise<T> { return xs[0]; }
export async function main(): Promise<void> {
    const n: i32 = 1;
    first([1, 2.5]);
    second(n, [1, 2.5]);
    ns.id([1, 2.5]);
    await af([1, 2.5]);
}
"#,
        ),
    ];
    let errors = check_program(&files).expect_err("mixed literal defaults must reject");
    assert_eq!(errors.len(), 4, "{errors:?}");
    for (error, line) in errors.iter().zip(8..=11) {
        assert_eq!(error.code, RuleCode::S008, "{error:?}");
        assert_eq!(error.pos.line, line, "{error:?}");
    }
}

#[test]
fn conflicting_literal_defaults_reject() {
    reject(r#"
function pair<T>(x: T, y: T): T { return x; }
export function main(): void { pair(1, 2.5); }
"#, "cannot infer type parameter `T` of `pair`: conflicting candidates `i32` and `f64`; use explicit type arguments");
}
