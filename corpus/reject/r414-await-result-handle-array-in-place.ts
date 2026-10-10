// corpus: reject/r414-await-result-handle-array-in-place
// purpose: An await result that holds handles is an origin; a read of its length in place drops the handles.
// exercises: async-handle, dropped-handle, await
// questions: compiler.md §188 rule 2, compiler.md §70, collisions.md C8
// tsc: accepts
// expected-error: S013, an async handle is dropped without any await of its completion
async function work(v: i32): Promise<i32> { return v; }
async function many(): Promise<Promise<i32>[]> { return [work(1), work(2)]; }
export async function main(): Promise<void> {
  print(`${(await many()).length}`);
  print("end");
}

// pin: 5149dfb3
// pin-dev-jit: Accepted; stdout `2`, `end`.
// pin-c-aot: Accepted; stdout `2`, `end`.
// pin-interpreter: Accepted; stdout `2`, `end`.
