// corpus: reject/r344-type-parameter-default
// purpose: Rejects a type-parameter default at its declaration.
// exercises: generic, type-parameter-default
// questions: collisions.md C24, compiler.md §154
// tsc: accepts
// expected-error: S100 at line 8, Rejects the type-parameter default.

function identity<T = i32>(value: T): T { return value; }
export function main(): void { identity<i32>(1); }
