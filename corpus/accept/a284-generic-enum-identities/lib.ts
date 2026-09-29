// corpus: accept/a284-generic-enum-identities/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { pick } from "./shared";
enum E { Value = 1 }
export function libRead(): E { return pick<E>(E.Value); }
