// corpus: reject/r343-generator-other-void-argument
// purpose: Rejects void outside the generator element slot.
// exercises: void, generator
// questions: collisions.md C21, compiler.md §151
// tsc: accepts
// expected-error: S100 at line 8, Rejects void outside the generator element slot.

function* tick(): Generator<void, void> { yield; }
export function main(): void {}
