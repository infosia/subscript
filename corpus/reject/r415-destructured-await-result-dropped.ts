// corpus: reject/r415-destructured-await-result-dropped
// purpose: A pattern binding carries the origin of its await result; no await through a binding drops the handles.
// exercises: async-handle, dropped-handle, await, binding-pattern
// questions: compiler.md §188 rule 2, compiler.md §107, collisions.md C8
// tsc: accepts
// expected-error: S013, an async handle is dropped without any await of its completion
async function work(v: i32): Promise<i32> { return v; }
async function many(): Promise<Promise<i32>[]> { return [work(1), work(2)]; }
export async function main(): Promise<void> {
  const [first] = await many();
  print("end");
}

// pin: 5149dfb3
// pin-dev-jit: Accepted; stdout `end`.
// pin-c-aot: Accepted; stdout `end`.
// pin-interpreter: Accepted; stdout `end`.
