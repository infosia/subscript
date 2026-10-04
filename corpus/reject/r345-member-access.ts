// corpus: reject/r345-member-access
// purpose: Enforces the member modifier contract.
// exercises: member-modifiers
// questions: compiler.md §155
// tsc: rejects TS2341
// expected-error: S100 at line 9, Rejects a private member read outside its class.

class A { private x: i32 = 1; }
export function main(): void { print(`${new A().x}`); }
