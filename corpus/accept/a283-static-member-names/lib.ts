// corpus: accept/a283-static-member-names/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

class Box { static value: i32 = 100; static read(): i32 { return Box.value; } }
export function libRead(): i32 { return Box.read(); }
