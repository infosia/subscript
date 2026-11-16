// corpus: reject/r521-loop-break-finalizer
// purpose: A loop break carries the finalizer's nullable store.
// exercises: loop-exit, break, finally, nullable-local
// questions: compiler.md §315 rule 1a, collisions.md C7
// tsc: accepts
// expected-error: S011 at line 17, e can be null after the finalizer

class A { x: i32 = 1; }
function mk(): A | null { return null; }
export function main(): void {
  let e: A | null = new A();
  if (e === null) { return; }
  for (let i = 0; i < 3; i++) {
    try { break; }
    finally { e = mk(); }
  }
  print(`${e.x}`);
}

// tsc-version: TypeScript 5.9.2; exit 0.
