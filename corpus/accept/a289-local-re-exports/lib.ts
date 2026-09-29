// corpus: accept/a289-local-re-exports/lib
// purpose: Provides the imported function for a local re-export.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

export function value(): i32 { return 2; }
