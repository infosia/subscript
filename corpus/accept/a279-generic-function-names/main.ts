// corpus: accept/a279-generic-function-names/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
function pick<T>(first: T, second: T): T { return second; }
export function main(): void { print(`main=${pick<i32>(1, 2)} lib=${libRead()}`); }
