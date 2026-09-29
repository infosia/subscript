// corpus: accept/a283-static-member-names/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
class Box { static value: i32 = 7; static read(): i32 { return Box.value; } }
export function main(): void { print(`main=${Box.read()} lib=${libRead()}`); }
