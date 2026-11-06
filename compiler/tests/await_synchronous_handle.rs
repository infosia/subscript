//! `await` reads the handle of a synchronous call (compiler.md §234).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process. The whole file took 0.04 s in a
//! debug build.

use subscript_compiler::{check_program, Diagnostic, SourceFile};

const DECLARATIONS: &str = r#"
class P { v: i32 = 7; }
class Buf<T> { items: T[] = []; }
async function v(n: i32): Promise<i32> { return n; }
async function af<T>(): Promise<Buf<T>> { return new Buf<T>(); }
function s(): Promise<i32> { return v(1); }
function g(): i32 { return 1; }
function saf<T>(): Promise<Buf<T>> { return af<T>(); }
class K {
  s(): Promise<i32> { return v(2); }
  f(): i32 { return 1; }
  static t(): Promise<i32> { return v(3); }
  static u(): i32 { return 4; }
  static tg<T>(x: T): Promise<T> { return ag<T>(x); }
  static ug<T>(x: T): T { return x; }
}
type H = Promise<i32>;
async function ag<T>(x: T): Promise<T> { return x; }
function first<T>(xs: T[]): T { return xs[0]; }
function aliased(n: i32): H { return v(n); }
class G {
  pick<T>(x: T): T { return x; }
  saf<T>(): Promise<Buf<T>> { return af<T>(); }
}
"#;

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source =
        format!("{DECLARATIONS}export async function main(): Promise<void> {{\n{body}\n}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

fn accepts(body: &str) {
    if let Err(errors) = check(body) {
        panic!("{body}: {errors:?}");
    }
}

fn rejects(body: &str, message: &str) {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    assert_eq!(errors[0].message, message, "{body}");
}

/// Rule 1: a synchronous function, method, or static method whose result
/// is a handle is awaited as a held handle is.
#[test]
fn await_reads_the_handle_of_a_synchronous_call() {
    for body in [
        "const a: i32 = await s();",
        "const b: i32 = await new K().s();",
        "const c: i32 = await K.t();",
        "print(`${await s()}`);",
        "await s();",
        "const d: Buf<P> = await saf<P>();",
    ] {
        accepts(body);
    }
}

/// Rule 1 control: the local form gives the same type, so a wrong
/// declaration gives the same text in both forms.
#[test]
fn the_awaited_call_has_the_type_of_the_local_form() {
    let message = "type mismatch: the initializer expects `string`, got `i32`";
    rejects(
        "const h: Promise<i32> = s(); const x: string = await h;",
        message,
    );
    rejects("const x: string = await s();", message);
}

/// Rule 2: a synchronous call whose result is not a handle keeps its text.
#[test]
fn a_synchronous_call_without_a_handle_keeps_its_text() {
    rejects("await g();", "`g` is synchronous and cannot be awaited");
    rejects(
        "await new K().f();",
        "method `f` is synchronous and cannot be awaited",
    );
    rejects("await K.u();", "class `K` used as a value");
}

/// Rule 4: a synchronous callee under `await` takes no contextual candidate.
#[test]
fn a_synchronous_callee_takes_no_context_from_await() {
    rejects(
        "const b: Buf<P> = await saf();",
        "cannot infer type parameter `T` of `saf`: no candidate; use explicit type arguments",
    );
}

/// Rule 1 reads the instantiated result type: a type argument or an alias
/// that gives `Promise<T>` decides as a written `Promise<T>` does. Each
/// path of the decision has a form here: a function, an instance method,
/// a static method, and a static generic method.
#[test]
fn the_instantiated_result_decides() {
    for body in [
        "const hs: Promise<i32>[] = [v(5)]; const a: i32 = await first<Promise<i32>>(hs);",
        "const b: i32 = await new G().pick<Promise<i32>>(v(8));",
        "const c: i32 = await aliased(9);",
        "const d: P = await K.tg<P>(new P());",
    ] {
        accepts(body);
    }
    // A generic callee whose instance does not give a handle keeps its text.
    rejects(
        "const x: i32 = await first<i32>([1]);",
        "`first<i32>` is synchronous and cannot be awaited",
    );
    rejects(
        "const x: i32 = await new G().pick<i32>(1);",
        "method `pick<i32>` is synchronous and cannot be awaited",
    );
    rejects("await K.ug<i32>(1);", "class `K` used as a value");
}

/// Rule 4 for a method: a synchronous generic method under `await` takes
/// no contextual candidate.
#[test]
fn a_synchronous_method_takes_no_context_from_await() {
    rejects(
        "const g: G = new G(); const a: Buf<P> = await g.saf();",
        "cannot infer type parameter `T` of `G.saf`: no candidate; use explicit type arguments",
    );
    accepts("const g: G = new G(); const a: Buf<P> = await g.saf<P>();");
}
