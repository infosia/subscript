// corpus: accept/a288-re-export-kinds/bridge
// purpose: Renames every declaration kind through the first re-export link.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

export { f as call, Box as Crate, count as total, bump as increment,
  Kind as Choice, Label as Status, pick as select, Holder as Container } from "./lib";
