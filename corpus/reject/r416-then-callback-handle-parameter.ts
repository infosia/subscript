// corpus: reject/r416-then-callback-handle-parameter
// purpose: A then callback parameter that holds handles is an origin; a callback that reads only its length drops them.
// exercises: async-handle, dropped-handle, Promise.then, lambda
// questions: compiler.md §188 rule 2, compiler.md §186, collisions.md C8
// tsc: accepts
// expected-error: S013, an async handle is dropped without any await of its completion
async function work(v: i32): Promise<i32> { return v; }
async function many(): Promise<Promise<i32>[]> { return [work(1), work(2)]; }
export async function main(): Promise<void> {
  const n = await many().then((xs: Promise<i32>[]): i32 => xs.length);
  print(`${n} end`);
}

// pin: 5149dfb3
// pin-dev-jit: Accepted; stdout `2 end`.
// pin-c-aot: Accepted; stdout `2 end`.
// pin-interpreter: Accepted; stdout `2 end`.
