// corpus: accept/a279-generic-function-names/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

function pick<T>(first: T, second: T): T { return first; }
export function libRead(): i32 { return pick<i32>(1, 2); }
