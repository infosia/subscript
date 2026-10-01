// corpus: reject/r288-growing-instance-chain
// purpose: Rejects a generic instance chain that grows without bound.
// exercises: generic-function, generic-class, polymorphic-recursion
// questions: compiler.md §140, C19
// tsc: accepts
// collision: C19
// expected-error: S100 at line 11, the request for nest<W<T>> (C19)
class W<T> { v: T; constructor(v: T) { this.v = v; } }
function nest<T>(x: T, n: i32): i32 {
  if (n <= 0) { return 0; }
  return 1 + nest<W<T>>(new W<T>(x), n - 1);
}
export function main(): void { print(`${nest<i32>(1, 3)}`); }
