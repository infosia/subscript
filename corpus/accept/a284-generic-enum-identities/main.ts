// corpus: accept/a284-generic-enum-identities/main
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { pick } from "./shared";
import { libRead } from "./lib";
enum E { Value = 9 }
export function main(): void { print(`main=${pick<E>(E.Value)} lib=${libRead()}`); }
