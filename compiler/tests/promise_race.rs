//! `Promise.race` at the checker (compiler.md §350 rules 1 and 2).
use subscript_compiler::{check_program, divergence::Divergence, Diagnostic, RuleCode, SourceFile};

const LEAVES: &str = "async function f(): Promise<i32> { return 1; }
async function g(): Promise<i32> { return 2; }
async function quiet(): Promise<void> {}
class Box { n: i32; constructor(n: i32) { this.n = n; } }
async function boxed(): Promise<Box> { return new Box(3); }
async function maybe(): Promise<Box | null> { return null; }
async function text(): Promise<string> { return \"s\"; }
async function plain(): Promise<i32[]> { const xs: i32[] = [await f()]; return xs; }
";

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!("{LEAVES}export async function main(): Promise<void> {{ {body} }}");
    check_program(&[SourceFile::new("race.ts", source)]).map(|_| ())
}

fn accept(body: &str) {
    if let Err(errors) = check(body) {
        panic!("{body}: {errors:?}");
    }
}

fn reject(body: &str) -> Vec<Diagnostic> {
    check(body).expect_err(body)
}

/// Rule 1: the result type is the element handle type. Control: the
/// `Promise.all` result type does not fit a race.
#[test]
fn race_has_the_element_handle_type() {
    accept("const r: Promise<i32> = Promise.race([f(), g()]); await r;");
    accept("const x: i32 = await Promise.race([f(), g()]);");
    assert!(!reject("const r: Promise<i32[]> = Promise.race([f(), g()]); await r;").is_empty());
    accept("const r: Promise<i32[]> = Promise.all([f(), g()]); await r;");
}

#[test]
fn race_accepts_the_held_value_and_class_forms() {
    accept("const jobs: Promise<i32>[] = [f(), g()]; const r = Promise.race(jobs); const x: i32 = await r;");
    accept("const b: Box = await Promise.race([boxed(), boxed()]); print(`${b.n}`);");
    accept("const b: Box | null = await Promise.race([maybe()]); print(`${b == null}`);");
    accept("const s: string = await Promise.race([text()]); print(s);");
}

/// A `void` race is a `Promise<void>` and enters a task group. Control:
/// a `void[]` value of `Promise.all` stays rejected.
#[test]
fn void_race_is_a_void_handle() {
    accept("const jobs: Promise<void>[] = [quiet(), quiet()]; await Promise.race(jobs);");
    accept("const jobs: Promise<void>[] = [quiet()]; const r: Promise<void> = Promise.race(jobs); await r;");
    accept("const g = new TaskGroup(); const jobs: Promise<void>[] = [quiet()]; g.add(Promise.race(jobs)); await g.join();");
    assert!(reject("const jobs: Promise<void>[] = [quiet()]; const xs = await Promise.all(jobs); print(`${xs.length}`);")
        .iter()
        .any(|error| error.divergence == Some(Divergence::PromiseAllVoidValue)));
}

/// Rule 1: a dropped race handle is S013. Control: the awaited statement.
#[test]
fn dropped_race_is_s013() {
    assert!(reject("Promise.race([f()]);")
        .iter()
        .any(|error| error.code == RuleCode::S013
            && error.message == "an async handle is dropped without any await of its completion"));
    accept("await Promise.race([f()]);");
}

/// Rules 1 and 2: each rejection names `Promise.race`.
#[test]
fn each_race_rejection_names_promise_race() {
    let cases = [
        (
            "const jobs: Promise<i32>[] = [f()]; await Promise.race<i32>(jobs);",
            "Promise.race takes no explicit type arguments",
            Some(Divergence::PromiseAllTypeArguments),
        ),
        (
            "const jobs: Promise<i32>[] = [f()]; await Promise.race(jobs, jobs);",
            "Promise.race requires one Promise<T>[] argument",
            None,
        ),
        (
            "await Promise.race();",
            "Promise.race requires one Promise<T>[] argument",
            None,
        ),
        (
            "const jobs: Promise<i32>[] = [f()]; await Promise.race(...[jobs]);",
            "Promise.race takes one Promise<T>[] argument, not a spread argument",
            Some(Divergence::ForOfSpreadCall),
        ),
        (
            "const xs: i32[] = [1]; await Promise.race(xs);",
            "Promise.race requires Promise<T>[]",
            Some(Divergence::PromiseAllInput),
        ),
        (
            "const x: i32 = 1; await Promise.race(x);",
            "Promise.race requires Promise<T>[]",
            Some(Divergence::PromiseAllInput),
        ),
    ];
    for (body, message, divergence) in cases {
        let errors = reject(body);
        assert!(
            errors.iter().any(|error| error.code == RuleCode::S013
                && error.message == message
                && error.divergence == divergence),
            "{body}: {errors:?}"
        );
    }
}

/// Rule 2: a result that holds a handle. Control: a byte result.
#[test]
fn counted_race_result_rejects_with_a_byte_control() {
    for counted in [false, true] {
        let (ty, body) = if counted {
            (
                "Promise<i32>[]",
                "const h: Promise<i32> = f(); const xs: Promise<i32>[] = [h]; await h; return xs;",
            )
        } else {
            ("f64[]", "const xs: f64[] = [7]; await f(); return xs;")
        };
        let source = format!(
            "{LEAVES}async function result(): Promise<{ty}> {{ {body} }}
             export async function main(): Promise<void> {{
                const jobs: Promise<{ty}>[] = [result()];
                await Promise.race(jobs);
             }}"
        );
        let checked = check_program(&[SourceFile::new("counted-race.ts", source)]);
        if counted {
            let errors = checked.expect_err("a counted result cannot enter the byte-copy store");
            assert!(errors.iter().any(|error| error.code == RuleCode::S013
                && error.divergence == Some(Divergence::PromiseAllCountedResult)
                && error.message == "Promise.race cannot store a counted result"));
        } else {
            checked.expect("a byte result uses the byte-copy store");
        }
    }
}

/// Rule 2 covers each type that `counted_type()` counts: a nested array
/// of handles and a generator. Control: an `i32[]` result checks.
#[test]
fn wider_counted_race_results_reject_with_a_plain_array_control() {
    let shapes = [
        (
            "Promise<i32>[][]",
            "const h: Promise<i32> = f(); const xs: Promise<i32>[][] = [[h]]; await h; return xs;",
            "const xs: Promise<i32>[][] = await Promise.race(jobs); print(`${await xs[0][0]}`);",
        ),
        (
            "Generator<i32>",
            "await f(); return numbers();",
            "const g: Generator<i32> = await Promise.race(jobs); for (const x of g) { print(`${x}`); }",
        ),
    ];
    for (ty, body, read) in shapes {
        let source = format!(
            "{LEAVES}function* numbers(): Generator<i32> {{ yield 1; }}
             async function result(): Promise<{ty}> {{ {body} }}
             export async function main(): Promise<void> {{
                const jobs: Promise<{ty}>[] = [result()];
                {read}
             }}"
        );
        let errors = check_program(&[SourceFile::new("counted-race.ts", source)]).expect_err(ty);
        assert!(
            errors.iter().any(|error| error.code == RuleCode::S013
                && error.divergence == Some(Divergence::PromiseAllCountedResult)
                && error.message == "Promise.race cannot store a counted result"),
            "{ty}: {errors:?}"
        );
    }
    accept(
        "const jobs: Promise<i32[]>[] = [plain()]; const xs: i32[] = await Promise.race(jobs); print(`${xs[0]}`);",
    );
}

/// §350.4: a literal of two result types keeps the array mismatch, and
/// the other statics keep their rejection.
#[test]
fn mixed_literal_and_other_statics_keep_their_rejections() {
    assert!(reject("await Promise.race([f(), text()]);")
        .iter()
        .any(|error| error.code == RuleCode::S100));
    for name in ["resolve", "any", "allSettled"] {
        let body = format!("const p = Promise.{name}(1);");
        assert!(
            reject(&body).iter().any(|error| error.message
                == format!("Promise static `Promise.{name}(...)` is not in the language")),
            "{name}"
        );
    }
}
