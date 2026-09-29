// corpus: accept/a285-mirror-constant-shadow/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

export function libRead(): u64 { return SUB_ACCESS_READ; }
