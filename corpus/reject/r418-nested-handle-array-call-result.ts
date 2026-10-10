// corpus: reject/r418-nested-handle-array-call-result
// purpose: A call result of Promise<i32>[][] is an origin; a read of its length in place drops the handles.
// exercises: async-handle, dropped-handle, array
// questions: compiler.md §188 rule 1, compiler.md §188 rule 2, collisions.md C8
// tsc: accepts
// expected-error: S013, an async handle is dropped without any await of its completion
async function work(v: i32): Promise<i32> { return v; }
function grid(): Promise<i32>[][] { return [[work(1)]]; }
export async function main(): Promise<void> {
  print(`${grid().length} end`);
}

// pin: 5149dfb3
// pin-dev-jit: Accepted; stdout `1 end`.
// pin-c-aot: Accepted; stdout `1 end`.
// pin-interpreter: Accepted; stdout `1 end`.
