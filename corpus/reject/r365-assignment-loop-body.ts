// corpus: reject/r365-assignment-loop-body
// purpose: Rejects a nullable read without a fact on every path.
// exercises: assignment, nullable-reference
// questions: compiler.md §159, collisions.md C7
// tsc: rejects TS18047
// expected-error: S011 at line 11, may be null here

class A { x: i32 = 1; }
function cond(): boolean { return true; }
export function main(): void {
  let a: A | null = null; for (let i: i32 = 0; i < 1; i++) { a = new A(); } a.x;
}
