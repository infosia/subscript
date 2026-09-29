// corpus: accept/a290-re-export-module-cycle/left
// purpose: Declares the left function and re-exports the right function through a module cycle.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

export function left(): i32 { return 1; }
export { right } from "./right";
