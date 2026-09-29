// corpus: accept/a282-class-names/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

class Box { value: i32 = 100; }
export function libRead(): i32 { return new Box().value; }
