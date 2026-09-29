// corpus: accept/a289-local-re-exports/main
// purpose: Calls both local re-export forms through the surface module.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

import { other, again } from "./surface";
export function main(): void { print(`local=${other()} imported=${again()}`); }
