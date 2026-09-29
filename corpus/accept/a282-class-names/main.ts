// corpus: accept/a282-class-names/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
class Box { value: i32 = 7; }
export function main(): void { print(`main=${new Box().value} lib=${libRead()}`); }
