// corpus: reject/r403-then-captures-let
// purpose: A then callback that captures a let binding is rejected.
// exercises: Promise.then, mutable-capture
// questions: compiler.md §186 rule 4, compiler.md §181 rule 1, collisions.md C8
// tsc: accepts
// expected-error: S009, which is not a `const` local
async function leaf(): Promise<i32> { return 1; }
export async function main(): Promise<void> {
  let n: i32 = 1;
  const h = leaf().then((v: i32): i32 => v + n);
  n = 10;
  print(`${await h}`);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 1 error, the only one S013 "Promise combinator `.then(...)` is not in the language" at 10:20; no S009.
// pin-c-aot: Exit 1; the same 1 error before C emission.
// pin-interpreter: Checker rejects with the same 1 error before lowering.
