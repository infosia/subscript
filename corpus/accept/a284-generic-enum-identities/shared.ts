// corpus: accept/a284-generic-enum-identities/shared
// purpose: Instantiates one template over distinct nominal enum types.
// exercises: generic-function, enum
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

export function pick<T>(value: T): T { return value; }
