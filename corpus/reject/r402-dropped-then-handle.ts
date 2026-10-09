// corpus: reject/r402-dropped-then-handle
// purpose: A then handle that no holder awaits is rejected.
// exercises: Promise.then, async-call-value
// questions: compiler.md §186 rule 1, compiler.md §70, collisions.md C8
// tsc: accepts
// expected-error: S013, an async handle is dropped without any await of its completion
async function leaf(): Promise<i32> { return 1; }
export async function main(): Promise<void> {
  const h = leaf();
  h.then((v: i32): void => { print(`${v}`); });
  await h;
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 1 error, the only one S013 "Promise combinator `.then(...)` is not in the language" at 10:5; the dropped-handle message is absent.
// pin-c-aot: Exit 1; the same 1 error before C emission.
// pin-interpreter: Checker rejects with the same 1 error before lowering.
