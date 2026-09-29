// corpus: accept/a280-generic-class-names/lib
// purpose: Keeps declarations separate across module scopes.
// exercises: module-import, declaration-identity
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

class Box<T> { value: T; constructor(first: T, second: T) { this.value = first; } }
export function libRead(): i32 { return new Box<i32>(100, 7).value; }
