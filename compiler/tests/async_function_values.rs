//! Async body facts and capture boundaries (§167).
use subscript_compiler::{
    check_program, hir::ExprKind, render_diagnostics, RuleCode, SourceFile, Type,
};

#[test]
fn deferred_initializers_keep_body_and_callable_results_with_synchronous_control() {
    for asynchronous in [false, true] {
        let flag = if asynchronous { "async " } else { "" };
        let body = if asynchronous {
            "await value()"
        } else {
            "value()"
        };
        let source = format!(
            "async function value(): Promise<i32> {{ return 7; }}
            const job: () => Promise<i32> = {flag}(): Promise<i32> => {body};
            export async function main(): Promise<void> {{ print(`${{await job()}}`); }}"
        );
        let module = check_program(&[SourceFile::new("facts.ts", source)]).expect("checks");
        let initializer = &module
            .globals
            .iter()
            .find(|global| global.name == "job")
            .expect("job")
            .init;
        let ExprKind::Lambda { is_async, ret, .. } = &initializer.kind else {
            panic!("lambda");
        };
        assert_eq!(*is_async, asynchronous);
        assert_eq!(
            *ret,
            if asynchronous {
                Type::I32
            } else {
                Type::AsyncHandle(Box::new(Type::I32))
            }
        );
        assert_eq!(
            initializer.ty.function_type().expect("callable").ret,
            Type::AsyncHandle(Box::new(Type::I32))
        );
    }
}

#[test]
fn local_parameter_and_receiver_captures_name_the_binding_with_clean_controls() {
    for (captured, clean, name) in [
        (
            "function f(): void { let n: i32 = 7; const job = async (): Promise<i32> => n; }",
            "function f(): void { const n: i32 = 7; const job = (): i32 => n; }",
            "n",
        ),
        (
            "function f(n: i32): void { const job = async (): Promise<i32> => n; }",
            "function f(n: i32): void { const job = async (n: i32): Promise<i32> => n; }",
            "n",
        ),
        (
            "class C { n: i32 = 7; f(): void { const job = async (): Promise<i32> => this.n; } }",
            "class C { n: i32 = 7; f(): void { const job = (): i32 => this.n; } }",
            "this",
        ),
    ] {
        let diagnostics =
            check_program(&[SourceFile::new("capture.ts", captured)]).expect_err("capture rejects");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::S009
                && diagnostic.message.contains(&format!("`{name}`"))
                && diagnostic.message.contains(if name == "this" {
                    "copy the receiver into a const"
                } else {
                    "copy it into a `const` first, or use a class with a field"
                })));
        check_program(&[SourceFile::new("control.ts", clean)]).expect("clean control checks");
    }
}

#[test]
fn async_body_requires_observation_with_awaited_control() {
    for observed in [false, true] {
        let call = if observed {
            "await value();"
        } else {
            "value();"
        };
        let source = format!("async function value(): Promise<i32> {{ return 7; }} function f(): void {{ const job = async (): Promise<void> => {{ {call} }}; }}");
        let result = check_program(&[SourceFile::new("observation.ts", source)]);
        if observed {
            result.expect("observed");
        } else {
            assert!(result
                .expect_err("dropped")
                .iter()
                .any(|d| d.code == RuleCode::S013));
        }
    }
}

#[test]
fn handle_returns_reject_with_explicit_await_controls() {
    for declaration in [
        "const job = async (h: Promise<i32>): Promise<i32> => BODY;",
        "const job: (h: Promise<i32>) => Promise<i32> = async (h) => BODY;",
        "const job = async (h: Promise<i32>) => BODY;",
        "const job = async (h: Promise<i32>): Promise<i32> => { return BODY; };",
        "async function job(h: Promise<i32>): Promise<i32> { return BODY; }",
    ] {
        let rejected = declaration.replace("BODY", "h");
        let diagnostics = check_program(&[SourceFile::new("return.ts", rejected)])
            .expect_err("handle return rejects");
        assert_eq!(diagnostics.len(), 1, "{declaration}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(
            diagnostics[0].message,
            "type mismatch: the return value expects `i32`, got `Promise<i32>`"
        );
        assert_eq!(
            diagnostics[0].divergence,
            Some(subscript_compiler::divergence::Divergence::AsyncReturnHandle)
        );
        check_program(&[SourceFile::new(
            "control.ts",
            declaration.replace("BODY", "await h"),
        )])
        .expect("explicit await checks");
    }
}

#[test]
fn nested_receiver_capture_rejects_with_synchronous_controls() {
    for depth in [1, 2] {
        let mut body = "this.n".to_string();
        for _ in 0..depth {
            body = format!("(() => {body})()");
        }
        for asynchronous in [false, true] {
            let flag = if asynchronous { "async " } else { "" };
            let result = if asynchronous { "Promise<i32>" } else { "i32" };
            let source = format!("class C {{ n: i32 = 7; f(): void {{ const job = {flag}(): {result} => {body}; }} }}");
            let checked = check_program(&[SourceFile::new("nested-this.ts", source)]);
            if asynchronous {
                assert!(checked
                    .expect_err("nested receiver capture rejects")
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code == RuleCode::S009
                            && diagnostic.message
                                == "an async arrow cannot capture `this` directly; copy the receiver into a const or use a class field with an async method"
                    }));
            } else {
                checked.expect("synchronous receiver capture checks");
            }
        }
    }
}

// Cost: four checker calls; no native build.
#[test]
fn owned_async_captures_keep_borrowed_transitive_and_map_restrictions() {
    let accepted = "async function value(): Promise<i32> { return 7; }
        function make(): () => Promise<i32> { const h = value(); return async (): Promise<i32> => { return await h; }; }";
    check_program(&[SourceFile::new("owned.ts", accepted)]).expect("owned counted capture escapes");
    let borrowed = "function make(): () => Promise<i32> { const n: i32 = 7; const g = (): i32 => n; return async (): Promise<i32> => { await Context.suspend(); return g(); }; }";
    let errors = check_program(&[SourceFile::new("borrowed.ts", borrowed)])
        .expect_err("transitive borrowed capture rejects");
    assert!(errors.iter().any(|error| error.code == RuleCode::S009));
    let started = "function start(): Promise<i32> { const n: i32 = 7; const g = (): i32 => n; const f = async (): Promise<i32> => { await Context.suspend(); return g(); }; return f(); }";
    let errors = check_program(&[SourceFile::new("started.ts", started)])
        .expect_err("started frame cannot retain a borrowed environment");
    assert!(errors
        .iter()
        .any(|error| error.code == RuleCode::S009
            && error.message.contains("owned async environment")));
    let map = "export function main(): void { const base: i32 = 7; const ids: i32[] = [1, 2]; const jobs = ids.map(async (id: i32): Promise<i32> => id + base); }";
    let errors =
        check_program(&[SourceFile::new("map.ts", map)]).expect_err("counted callback rejects");
    assert!(errors.iter().any(|error| error.code == RuleCode::S014));
    assert!(!errors.iter().any(|error| error.code == RuleCode::S009));
}

// Cost: thirteen checker calls and renders; no subprocess or native build.
#[test]
fn listed_diagnostics_render_the_contract_text() {
    let started = std::time::Instant::now();
    for (source, message, rule, ts, replacement, reason) in [
        ("function* values(): Generator<i32> { new TaskGroup(); yield 1; }\nexport function main(): void {}", "TaskGroup is not allowed in a generator body", "A generator body cannot use a TaskGroup because a dropped iterator has no scope exit.", "function* values(): Generator<i32> { new TaskGroup(); yield 1; }\nexport function main(): void {}", "async function values(): Promise<void> { const g = new TaskGroup(); await g.join(); }\nexport function main(): void {}", "A dropped generator does not execute a lexical scope exit. Put the group in an async function and await its join. (collisions.md C24)"),
        ("async function value(n: i32): Promise<i32> { return n; } async function probe(): Promise<void> { const f = async () => value(7); print(`${await f()}`); }\nexport function main(): void {}", "type mismatch: the return value expects `i32`, got `Promise<i32>`", "Constructs outside the decided language surface are rejected.", "async function f(h: Promise<i32>): Promise<i32> { return h; }", "async function f(h: Promise<i32>): Promise<i32> { return await h; }", "An async return carries its fulfilled value. The language has no implicit handle adoption. (compiler.md §167)"),
        ("function f(): void { let n: i32 = 1; const job = async (): Promise<i32> => n; }\nexport function main(): void {}", "async arrow captures mutable `n`; copy it into a `const` first, or use a class with a field", "An async arrow can own const captures. Copy a mutable value into a const or use a class field.", "function f(): void { let n: i32 = 1; const job = async (): Promise<i32> => n; }", "function f(): void { let n: i32 = 1; const copy = n; const job = async (): Promise<i32> => copy; }\nexport function main(): void {}", "An async arrow owns immutable captures. A mutable binding needs an explicit const copy or a class field. (collisions.md C24)"),
        ("async function probe(): Promise<void> { await ((): string => \"x\")(); }\nexport function main(): void {}", "await requires a call that returns an async handle", "Constructs outside the decided language surface are rejected.", " async function probe(): Promise<void> { await (() : i32 => 1)(); }\nexport function main(): void {}", "async function probe(): Promise<void> { (() : i32 => 1)(); }\nexport function main(): void {}", "The call returns a synchronous value. It supplies no async completion for an await. (compiler.md §167)"),
        ("function id<T>(x: T): T { return x; } function apply(f: (x: i32) => i32, x: i32): i32 { return f(x); } export function main(): void { apply(id, 3); }", "generic function `id` has no first-class value; call it directly or use a lambda", "Constructs outside the decided language surface are rejected.", "function id<T>(x:T):T{return x;} function apply<T>(f:(x:T)=>T,x:T):T{return f(x);} export function main():void { apply(id,3); }", "function id<T>(x: T): T { return x; } function apply(f: (x: i32) => i32, x: i32): i32 { return f(x); } export function main(): void { apply((x: i32): i32 => id<i32>(x), 3); }", "Generic function values require instantiation outside the admitted inference surface. (compiler.md §149.1)"),
        ("export function main(): void { const d = new Date(0); const g = d.getTime; }", "`getTime` may only be called, not read as a value (Q20)", "Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected.", "export function main(): void { const d = new Date(0); const g = d.getTime; }", "export function main(): void { const d = new Date(0); const g = (): i64 => d.getTime(); g(); }", "A Date method lowers to a direct operation. Use a lambda that calls the method on the Date value. (stdlib.md §3; collisions.md C24 row 12)"),
        ("function id(x: i32): i32 { return x; } export function main(): void { [1].map((x: i32): ((x: i32) => i32) => id); }", "`map` produces a `((i32) => i32)[]`; `(i32) => i32` is outside the supported element kinds (Q22)", "Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected.", "function id(x: i32): i32 { return x; } export function main(): void { [1].map((x: i32): ((x: i32) => i32) => id); }", "function id(x: i32): i32 { return x; } export function main(): void { const fs: ((x: i32) => i32)[] = []; for (const x of [1]) { fs.push(id); } }", "map does not support a function result. Use a typed array and push each function in a for-of loop. (stdlib.md §9)"),
        ("async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().then(null, (e: Error): i32 => 0); }", "`.then(...)` takes no `null` callback; `.catch(r)` takes a rejection callback alone", "Promise constructors and unsupported combinators are not in the language; every async handle must have an awaited completion.", "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().then(null, (e: Error): i32 => 0); }", "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch((e: Error): i32 => 0); const s: string = await leaf().then((x: i32): string => `${x}`); }", "then takes a fulfillment callback and an optional rejection callback, and no type arguments. Use catch for a rejection callback alone. (collisions.md C8; compiler.md §186)"),
        ("async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch<i32>((e: Error): i32 => 0); }", "`.catch(...)` takes no type arguments; the callback result gives the value type", "Promise constructors and unsupported combinators are not in the language; every async handle must have an awaited completion.", "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch<i32>((e: Error): i32 => 0); }", "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch((e: Error): i32 => 0); }", "catch takes one callback with an optional Error parameter, and no type arguments. (collisions.md C8; compiler.md §186)"),
        ("async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { return await leaf().then((v: i32): i32 => v + this.n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }", "a `.then(...)` callback cannot capture `this`; copy the needed field into a `const` first", "A reaction callback owns const captures. Copy the needed field into a const.", "async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { return await leaf().then((v: i32): i32 => v + this.n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }", "async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { const n = this.n; return await leaf().then((v: i32): i32 => v + n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }", "A reaction callback owns const captures. A direct this capture remains outside the accepted surface. (collisions.md C24; compiler.md §186)"),
        ("class C { n: i32 = 1; f(): void { const job = async (): Promise<i32> => this.n; } }\nexport function main(): void {}", "an async arrow cannot capture `this` directly; copy the receiver into a const or use a class field with an async method", "An async arrow owns const captures. A direct this capture remains outside the accepted surface.", "class C { n: i32 = 1; f(): void { const job = async (): Promise<i32> => this.n; } }\nexport function main(): void {}", "class C { n: i32 = 1; f(): void { const self = this; const job = async (): Promise<i32> => self.n; } }\nexport function main(): void {}", "The receiver must appear as an explicit const capture or a class field. (collisions.md C24)"),
        ("async function probe(): Promise<void> { const hs = [1].map(async (v: i32): Promise<i32> => v); await Promise.all(hs); }\nexport function main(): void {}", "`map` cannot carry a counted callback result (§171)", "Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected.", "async function probe(): Promise<void> { const hs = [1].map(async (v: i32): Promise<i32> => v); await Promise.all(hs); }\nexport function main(): void {}", "class Job { n: i32; constructor(n: i32) { this.n = n; } async run(): Promise<i32> { return this.n; } } async function probe(): Promise<void> { const hs: Promise<i32>[] = []; for (const v of [1]) { const job = new Job(v); hs.push(job.run()); } await Promise.all(hs); }\nexport function main(): void {}", "map cannot transfer a counted callback result. Use a for-of loop, push each handle, and await the handle array. (compiler.md §171)"),
        ("async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally(); }", "`.finally(...)` requires a callback", "Promise constructors and unsupported combinators are not in the language; every async handle must have an awaited completion.", "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally(); }", "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally((): void => {}); }", "finally takes one callback with no parameter, and no type arguments. (collisions.md C8; compiler.md §180, §186)"),
    ] {
        let files = [SourceFile::entry("main.ts", source)];
        let diagnostics = check_program(&files).expect_err("listed form rejects");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].message, message);
        let rendered = render_diagnostics(&files, &diagnostics);
        for expected in [format!("= rule: {rule}"), format!("= why: {reason}")] {
            assert!(rendered.contains(&expected), "{rendered}");
        }
        for (heading, text) in [("TypeScript accepts", ts), ("subscript", replacement)] {
            let expected = format!("= {heading}:\n{}", text.lines().map(|line| format!("  |   {line}\n")).collect::<String>());
            assert!(rendered.contains(&expected), "{rendered}");
        }
    }
    eprintln!(
        "s182 text cost: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

// Cost: five checker calls; rejected consumers and constrained-generic controls.
#[test]
fn rejected_handle_body_keeps_the_intended_consumer_type() {
    let started = std::time::Instant::now();
    for (consumer, count) in [
        ("print(`${await f()}`);", 1),
        ("const s: string = await f();", 2),
    ] {
        let source = format!("async function value(n: i32): Promise<i32> {{ return n; }} async function probe(): Promise<void> {{ const f = async () => value(7); {consumer} }} export function main(): void {{}}");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)])
            .expect_err("handle body rejects");
        assert_eq!(diagnostics.len(), count, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "type mismatch: the return value expects `i32`, got `Promise<i32>`"
        );
        if count == 2 {
            assert!(
                diagnostics[1]
                    .message
                    .contains("expects `string`, got `i32`"),
                "{diagnostics:?}"
            );
        }
    }
    for body in [
        "const g = async (v: T) => v; const y: T = await g(x); return y;",
        "const copy: T = x; const g = async () => copy; const typed: () => Promise<T> = g; const y: T = await typed(); return y;",
    ] {
        let source = format!("class Base {{ n: i32 = 1; }} async function f<T extends Base>(x: T): Promise<T> {{ {body} }} export async function main(): Promise<void> {{ const b = await f<Base>(new Base()); print(`${{b.n}}`); }}");
        check_program(&[SourceFile::entry("main.ts", source)])
            .expect("constrained generic retains its written return type");
    }
    let source = "async function f<T extends Promise<i32>>(x: T): Promise<void> { const g = async (v: T) => v; const n: i32 = await g(x); print(`${n}`); } export function main(): void {}";
    let diagnostics = check_program(&[SourceFile::entry("main.ts", source)])
        .expect_err("a Promise-constrained generic body rejects implicit handle adoption");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "type mismatch: the return value expects `i32`, got `T`"
    );
    eprintln!(
        "s182 cascade cost: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}
