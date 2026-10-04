// corpus: reject/r348-readonly-write
// purpose: Enforces the member modifier contract.
// exercises: member-modifiers
// questions: compiler.md §155
// tsc: rejects TS2540
// expected-error: S100 at line 9, Rejects a readonly field write outside its constructor.

class A { readonly x: i32 = 1; }
export function main(): void { const a: A = new A(); a.x = 2; }
