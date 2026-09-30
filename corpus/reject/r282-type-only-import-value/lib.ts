// corpus: reject/r282-type-only-import-value/lib
// purpose: Exports the class that the entry imports as a type only.
// exercises: module-export
// questions: compiler.md §134

export class Box { value: i32 = 1; }
