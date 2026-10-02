// corpus: reject/r342-void-generator-value-read
// purpose: Rejects a void generator value as an operand.
// exercises: void
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 10, Rejects a void generator value as an operand.

function* tick(): Generator<void> { yield; }
function sink<T>(v: T): void {}
export function main(): void { const g = tick(); g.next(); sink(g.next().value); }
