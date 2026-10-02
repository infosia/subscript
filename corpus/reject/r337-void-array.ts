// corpus: reject/r337-void-array
// purpose: Rejects a void array element.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 8, Rejects a void array element.

export function main(): void { const out: void[] = []; }
