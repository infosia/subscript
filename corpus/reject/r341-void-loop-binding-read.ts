// corpus: reject/r341-void-loop-binding-read
// purpose: Rejects a void loop binding as an operand.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 10, Rejects a void loop binding as an operand.

function* tick(): Generator<void> { yield; }
function sink<T>(v: T): void {}
export function main(): void { for (const x of tick()) { sink(x); } }
