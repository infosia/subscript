// corpus: accept/a280-generic-class-names/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
class Box<T> { value: T; constructor(first: T, second: T) { this.value = second; } }
export function main(): void { print(`main=${new Box<i32>(100, 7).value} lib=${libRead()}`); }
