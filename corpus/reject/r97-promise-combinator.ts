// corpus: reject/r97-promise-combinator
// purpose: Rejects a Promise combinator form outside then, catch, and finally with callbacks.
// exercises: Promise.then, async-call-value
// questions: Q34, C8, compiler.md §186
// tsc: accepts
// expected-error: S013 at the `null` callback of `.then`
async function leaf(): Promise<i32> {
  return 1;
}

export async function main(): Promise<void> {
  print(`${await leaf().then(null, (e: Error): i32 => 0)}`);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 2 errors, the first S018 "type `Promise<i32>` has no async method `then`" at 12:25.
// pin-c-aot: Exit 1; the same 2 errors before C emission.
// pin-interpreter: Checker rejects with the same 2 errors before lowering.
