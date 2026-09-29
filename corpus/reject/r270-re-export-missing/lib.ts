// corpus: reject/r270-re-export-missing/lib
// purpose: Exports a control name while leaving the requested name absent.
// exercises: module-export, module-import
// questions: compiler.md §128

export const present: i32 = 1;
