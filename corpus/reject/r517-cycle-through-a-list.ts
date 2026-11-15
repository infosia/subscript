// corpus: reject/r517-cycle-through-a-list
// purpose: Rejects a result cycle through an inferred list element.
// exercises: lambda, inference, type-parameter
// questions: compiler.md §306
// tsc: rejects TS7023
// expected-error: S100 at line 8, Rejects a result cycle through an inferred list element.

const a = () => { return table[0](); };
const table = [a];
export function main(): void {}
