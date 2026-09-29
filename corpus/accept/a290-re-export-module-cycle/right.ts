// corpus: accept/a290-re-export-module-cycle/right
// purpose: Declares the right function and re-exports the left function through a module cycle.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

export function right(): i32 { return 2; }
export { left } from "./left";
