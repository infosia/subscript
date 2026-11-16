// corpus: reject/r529-finalizer-call
// purpose: A finalizer jump cause selects the nullable call diagnostic.
// exercises: finally, loop-break, nullable-call
// questions: compiler.md §315 rule 1b, collisions.md C7
// tsc: accepts
// expected-error: S100 at line 12, the finalizer can end the local fact


class A { x: i32 = 1; }
function mk(): A | null { return null; }
function take(a: A): void {}
function id(x: i32): i32 { return x; } function f(next: ((x: i32) => i32) | null): void { let cb: ((x: i32) => i32) | null = id; for (let i = 0; i < 3; i++) { try { break; } finally { cb = next; } } print(`${cb(1)}`); }

// tsc-version: TypeScript 5.9.2; exit 0.
