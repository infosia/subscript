// corpus: reject/r527-finalizer-normal-branch-join
// purpose: A finalizer cannot supply a fact absent from a jump or a normal edge.
// exercises: finally, jump, nullable-local, flow-join
// questions: compiler.md §315 rules 1a and 1b
// tsc: rejects TS18047
// expected-error: S011 at line 10, the local can be null

class A { x: i32 = 1; }
function mk(): A | null { return null; }
function f(e: A | null, k: i32): void { if (e === null) { return; } if (k === 0) { while (true) { try { break; } finally { e = mk(); } } } else { e = mk(); } print(`${e.x}`); }

// tsc-version: TypeScript 5.9.2; exit 2.
