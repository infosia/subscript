// corpus: reject/r526-loop-finalizer-mixed-back-edges
// purpose: A finalizer cannot supply a fact absent from a jump or a normal edge.
// exercises: finally, jump, nullable-local, flow-join
// questions: compiler.md §315 rules 1a and 1b
// tsc: rejects TS18047
// expected-error: S011 at line 10, the local can be null

class A { x: i32 = 1; }
function mk(): A | null { return null; }
function f(e: A | null): void { if (e === null) { return; } let i: i32 = 0; while (true) { print(`${e.x}`); i++; if (i > 2) { break; } try { if (i === 1) { continue; } } finally { e = mk(); } } }

// tsc-version: TypeScript 5.9.2; exit 2.
