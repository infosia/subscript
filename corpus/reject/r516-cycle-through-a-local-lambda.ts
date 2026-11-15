// corpus: reject/r516-cycle-through-a-local-lambda
// purpose: Rejects a result cycle through a local lambda.
// exercises: lambda, inference, type-parameter
// questions: compiler.md §306
// tsc: rejects TS7023
// expected-error: S100 at line 8, Rejects a result cycle through a local lambda.

const a = () => { const inner = () => b(); return inner(); };
const b = () => { return a(); };
export function main(): void {}
