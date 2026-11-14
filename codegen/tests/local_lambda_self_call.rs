//! Runtime and environment checks for local self-calls (compiler.md §284).
//! Cost: both tests total six checker calls, six lowerings, and two native runs per tier.
//! Measured: both tests complete in 0.60 seconds (debug, Apple Silicon).

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn self_calls_keep_aggregate_captures_owned_environments_and_async_start() {
    let source = r#"
class Value { n: i32 = 7; }
async function tick(): Promise<void> {}
async function leaf(): Promise<i32> { return 9; }
function make(): (n: i32) => i32 {
    const text = "owned";
    const rec = (n: i32): i32 => { print(text); if (n > 0) return rec(n - 1); return 8; };
    return rec;
}
export async function main(): Promise<void> {
    const pair: FixedArray<i32, 2> = [3, 4];
    const text = "borrowed";
    const value = new Value();
    const callback = (): i32 => value.n;
    const rec = (n: i32): FixedArray<i32, 2> => {
        print(`${text}:${callback()}`);
        if (n > 0) return rec(n - 1);
        return pair;
    };
    const result = rec(1);
    print(`${result[0]}:${result[1]}`);
    const owned = make(); print(`${owned(1)}`);
    const copy = value;
    const asynchronous = async (n: i32): Promise<i32> => {
        await tick();
        const first = copy.n;
        if (n > 0) return await asynchronous(n - 1);
        return first;
    };
    print(`${await asynchronous(2)}`);
    const promise = (n: i32): Promise<i32> => {
        if (n > 0) return promise(n - 1);
        return leaf();
    };
    print(`${await promise(2)}`);
}
"#;
    let files = [SourceFile::new("self-call.ts", source)];
    let expected = b"borrowed:7\nborrowed:7\n3:4\nowned\nowned\n8\n7\n9\n";
    let hir = check_program(&files).expect("self-call source checks");
    let lir = lower_module(&hir).expect("self-call source lowers");
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("development tier"), expected);
    assert_eq!(run_c_aot(&files).expect("ship tier"), expected);
}

/// compiler.md §288 rules 2, 5, and 6: defaults share declaration-call
/// ordering, exceptions, and ownership, including an owned self-call.
#[test]
fn named_defaults_preserve_raises_handle_counts_and_owned_self_environments() {
    let source = r#"
function before(): i32 { return top(); }
const seed = (n: i32 = 5): i32 => n;
const top = (n: i32 = seed()): i32 => n;
const callable = (f: () => i32 = (): i32 => 7): i32 => f();
const initial = callable();
function fail(): i32 { print("default"); throw new Error("failed"); }
async function leaf(): Promise<i32> { return 3; }
function make(): (n: i32, s: string) => i32 {
    const base = 4;
    const rec = (n: i32, s: string = "own"): i32 => {
        print(`${s}${n}:${base}`);
        if (n > 0) return rec(n - 1);
        return base;
    };
    return rec;
}
export async function main(): Promise<void> {
    print(`${before()}:${initial}`);
    const owned = make(); print(`${owned(2, "arg")}`);
    const job = async (n: i32 = fail()): Promise<i32> => n;
    try { print(`${await job()}`); } catch (e) { if (e instanceof Error) print(e.message); }
    print(`${await job(8)}`);
    const use = async (first: Promise<i32>, second: Promise<i32> = first): Promise<i32> => {
        return (await first) + (await second);
    };
    print(`${await use(leaf())}`);
    const callback = (n: i32, f: () => i32 = (): i32 => 9): i32 => f();
    print(`${callback(9)}`);
}
"#;
    let files = [SourceFile::new("named-defaults.ts", source)];
    let expected = b"5:7\narg2:4\nown1:4\nown0:4\n4\ndefault\nfailed\n8\n6\n9\n";
    let hir = check_program(&files).expect("named defaults check");
    let lir = lower_module(&hir).expect("named defaults lower");
    subscript_codegen::lir::verify_module(&lir).expect("named defaults verify");
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("development tier"), expected);
    assert_eq!(run_c_aot(&files).expect("ship tier"), expected);
}
