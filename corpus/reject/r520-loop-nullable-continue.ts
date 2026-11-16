// corpus: reject/r520-loop-nullable-continue
// purpose: A nullable continue edge ends the loop-head fact.
// exercises: loop-head, assignment, continue
// questions: compiler.md §315, collisions.md C7
// tsc: rejects TS18047
// expected-error: S011 at line 14, e can be null at the loop head

class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
export function main(): void {
  let e: A | null = mk();
  if (e === null) { return; }
  for (let i = 0; i < 3; i++) {
    print(`${e.x}`); e = new A(); if (i === 1) { e = mk(); continue; }
  }
}
