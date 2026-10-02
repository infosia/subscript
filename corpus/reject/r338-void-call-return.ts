// corpus: reject/r338-void-call-return
// purpose: Rejects a void call as a return operand.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 9, Rejects a void call as a return operand.

function f(): void {}
function g(): void { return f(); }
export function main(): void { g(); }
