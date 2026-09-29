// corpus: accept/a288-re-export-kinds/surface
// purpose: Exposes the second re-export link without local bindings.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

export { call, Crate, total, increment, Choice, Status, select, Container } from "./bridge";
