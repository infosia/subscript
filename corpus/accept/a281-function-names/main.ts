// corpus: accept/a281-function-names/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
export function read(): i32 { return 7; }
export function main(): void { print(`main=${read()} lib=${libRead()}`); }
