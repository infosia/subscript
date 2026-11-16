// corpus: reject/r528-finalizer-nominal
// purpose: A finalizer jump cause selects the nullable nominal diagnostic.
// exercises: finally, loop-break, nullable-nominal
// questions: compiler.md §315 rule 1b, collisions.md C7
// tsc: accepts
// expected-error: S005 at line 12, the finalizer can end the local fact


class A { x: i32 = 1; }
function mk(): A | null { return null; }
function take(a: A): void {}
function f(): void { let e: A | null = new A(); for (let i = 0; i < 3; i++) { try { break; } finally { e = mk(); } } take(e); }

// tsc-version: TypeScript 5.9.2; exit 0.
