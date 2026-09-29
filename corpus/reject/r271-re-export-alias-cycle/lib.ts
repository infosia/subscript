// corpus: reject/r271-re-export-alias-cycle/lib
// purpose: Returns the alias dependency to the entry without a declaration.
// exercises: module-export, module-import
// questions: compiler.md §128

export { x } from "./main";
