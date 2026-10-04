// corpus: reject/r351-initializer-type-cycle
// purpose: Rejects a cycle in an inferred lambda result.
// exercises: module-global, lambda, inference-cycle
// questions: compiler.md §156 rule 3
// tsc: rejects TS7023
// expected-error: S100 at line 8, Rejects the initializer type cycle.

const recurse = (n: i32) => n === 0 ? 0 : recurse(n - 1);
export function main(): void {}
