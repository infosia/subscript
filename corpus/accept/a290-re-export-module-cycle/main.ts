// corpus: accept/a290-re-export-module-cycle/main
// purpose: Calls declarations reached through a module cycle with no alias cycle.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

import { left, right } from "./left";
export function main(): void { print(`left=${left()} right=${right()}`); }
