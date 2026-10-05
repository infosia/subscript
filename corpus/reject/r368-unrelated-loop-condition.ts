// corpus: reject/r368-unrelated-loop-condition
// purpose: Rejects a nullable read without a fact on every path.
// exercises: nullable-reference, narrowing-flow
// questions: compiler.md §162, collisions.md C7
// tsc: rejects TS18047
// expected-error: S011 at line 12, may be null here

class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
function cond(): boolean { return true; }
export function main(): void {
  let a = mk(); while (cond()) { a = mk(); } a.x;
}
