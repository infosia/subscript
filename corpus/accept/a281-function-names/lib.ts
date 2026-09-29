// corpus: accept/a281-function-names/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

export function read(): i32 { return 100; }
export function libRead(): i32 { return read(); }
