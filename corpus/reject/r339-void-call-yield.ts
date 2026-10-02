// corpus: reject/r339-void-call-yield
// purpose: Rejects a void call as a yield operand.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 9, Rejects a void call as a yield operand.

function f(): void {}
function* g(): Generator<void> { yield f(); }
export function main(): void {}
