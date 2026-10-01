// corpus: accept/a304-constant-instance-chain
// purpose: Accepts a finite recursive chain with a constant nested type argument.
// exercises: generic-function, generic-class, polymorphic-recursion
// questions: compiler.md §140, C19
// tsc: accepts; js-comparable: yes
class W<T> { v: T; constructor(v: T) { this.v = v; } }
function f<T>(n: i32): i32 {
  if (n <= 0) { return 0; }
  return 1 + f<W<i32>>(n - 1);
}
export function main(): void { print(`${f<i32>(3)}`); }
