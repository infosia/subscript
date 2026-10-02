// corpus: reject/r336-void-parameter
// purpose: Rejects a void parameter.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 8, Rejects a void parameter.

function sink(v: void): void {}
export function main(): void {}
