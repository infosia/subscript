// corpus: accept/a278-module-global-names/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

let x: i32 = 100;
export function libBump(): i32 { x += 1; return x; }
