// corpus: reject/r276-rejected-declaration-export/lib
// purpose: Retains both rejected binding names in the export graph.
// exercises: module-export, binding-pattern
// questions: compiler.md §128, §107.4
// tsc: accepts
// expected-error: S100 at line 8, the module-level binding pattern

export const [a, b] = [1, 2];
