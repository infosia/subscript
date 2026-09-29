// corpus: accept/a285-mirror-constant-shadow/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
const SUB_ACCESS_READ: i32 = 7;
export function main(): void { print(`main=${SUB_ACCESS_READ} lib=${libRead()}`); }
