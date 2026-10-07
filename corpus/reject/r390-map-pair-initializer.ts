// corpus: reject/r390-map-pair-initializer
// purpose: Pin the fact split of compiler section 173.
// exercises: rejection-class, divergence-block
// questions: compiler.md §173
// tsc: accepts
// expected-error: S014: `new Map(iterable)` requires a pair element, but the language has no tuple type.

export function main(): void { const m = new Map<string, i32>([["a", 1]]); print(`${m.size}`); }
