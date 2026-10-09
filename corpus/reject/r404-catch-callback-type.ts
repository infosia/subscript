// corpus: reject/r404-catch-callback-type
// purpose: A catch callback whose result is not the handle value type is rejected.
// exercises: Promise.catch, union-result
// questions: compiler.md §186 rule 6, collisions.md C8
// tsc: accepts
// expected-error: S013, a `catch` callback must return `i32`
async function leaf(): Promise<i32> { return 1; }
export async function main(): Promise<void> {
  const r = await leaf().catch((e: Error): string => e.message);
  print(`${r}`);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 2 errors, the first S018 "type `Promise<i32>` has no async method `catch`" at 9:26; no S013 divergence.
// pin-c-aot: Exit 1; the same 2 errors before C emission.
// pin-interpreter: Checker rejects with the same 2 errors before lowering.
