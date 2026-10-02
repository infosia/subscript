// corpus: reject/r335-annotated-void-binding
// purpose: Rejects an annotated void binding.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 9, Rejects an annotated void binding.

function f(): void {}
export function main(): void { let w: void = f(); }
