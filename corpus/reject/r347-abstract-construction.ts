// corpus: reject/r347-abstract-construction
// purpose: Enforces the member modifier contract.
// exercises: member-modifiers
// questions: compiler.md §155
// tsc: rejects TS2511
// expected-error: S100 at line 9, Rejects construction of an abstract class.

abstract class A { x: i32 = 1; }
export function main(): void { new A(); }
