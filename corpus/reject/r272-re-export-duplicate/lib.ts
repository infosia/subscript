// corpus: reject/r272-re-export-duplicate/lib
// purpose: Supplies two distinct declarations for the duplicate export name.
// exercises: module-export, module-import
// questions: compiler.md §128

export const a: i32 = 1;
export const b: i32 = 2;
