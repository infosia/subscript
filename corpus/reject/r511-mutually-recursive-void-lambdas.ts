// corpus: reject/r511-mutually-recursive-void-lambdas
// purpose: Rejects a cycle in inferred void lambda results.
// exercises: module-global, lambda, inference-cycle
// questions: compiler.md §290 rule 1, §156 rule 3
// tsc: rejects TS7023
// expected-error: S100 at line 8, Rejects the initializer type cycle.

const a = () => { return b(); };
const b = () => { return a(); };
export function main(): void {}
