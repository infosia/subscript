// corpus: reject/r274-type-only-export/lib
// purpose: Supplies a string alias and a class for the type-only export list.
// exercises: module-export
// questions: compiler.md §128, C18

export type Label = "ready" | "done";
export class Box { value: i32 = 4; }
