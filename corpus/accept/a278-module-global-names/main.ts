// corpus: accept/a278-module-global-names/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libBump } from "./lib";
let x: i32 = 7;
export function main(): void { const value = libBump(); print(`main=${x} lib=${value}`); }
