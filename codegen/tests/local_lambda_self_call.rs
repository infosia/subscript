//! Runtime and environment checks for local self-calls (compiler.md §284).
//! Cost: three checker calls, three lowerings, and one native run per tier.

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
