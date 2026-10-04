// corpus: reject/r346-constructor-access
// purpose: Enforces the member modifier contract.
// exercises: member-modifiers
// questions: compiler.md §155
// tsc: rejects TS2673
// expected-error: S100 at line 9, Rejects a private constructor call outside its class.

class A { private constructor() {} }
export function main(): void { new A(); }
