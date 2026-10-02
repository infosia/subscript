// corpus: reject/r340-void-call-argument
// purpose: Rejects a void call as an argument.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 10, Rejects a void call as an argument.

function f(): void {}
function sink<T>(v: T): void {}
export function main(): void { sink(f()); }
