// corpus: reject/r522-switch-break-finalizer
// purpose: A switch break carries the finalizer's nullable store.
// exercises: switch-exit, break, finally, nullable-local
// questions: compiler.md §315 rule 1a, collisions.md C7
// tsc: accepts
// expected-error: S011 at line 18, e can be null after the finalizer

class A { x: i32 = 1; }
function mk(): A | null { return null; }
export function main(): void {
  let e: A | null = new A();
  if (e === null) { return; }
  const k: i32 = 1;
  switch (k) {
    case 1: try { break; }
            finally { e = mk(); }
  }
  print(`${e.x}`);
}

// tsc-version: TypeScript 5.9.2; exit 0.
