// corpus: reject/r31-array-reduce-noinit
// purpose: Rejects `reduce` without an init: the lib's arity-overloaded
//          no-init form silently changes meaning (the first element
//          becomes the seed); the init is required (Q22).
// exercises: rejected-array-subset, array-methods
// questions: Q22
// tsc: accepts
// expected-error: reduce requires an explicit init
export function main(): void {
  const xs: i32[] = [3, 1, 2];
  const total: i32 = xs.reduce((acc: i32, v: i32): i32 => acc + v);
  print(`${total}`);
}
