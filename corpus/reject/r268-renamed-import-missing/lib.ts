// corpus: reject/r268-renamed-import-missing/lib
// purpose: Exports the local spelling but not the imported spelling.
// exercises: module-export
// questions: compiler.md §126

export function present(): i32 { return 1; }
