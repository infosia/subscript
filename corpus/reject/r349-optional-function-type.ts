// corpus: reject/r349-optional-function-type
// purpose: Enforces the member modifier contract.
// exercises: member-modifiers
// questions: compiler.md §155, collisions.md C7
// tsc: accepts
// expected-error: S012 at line 8, Rejects an optional parameter in a function type.

function apply(f: (a: i32, b?: i32) => void): void { f(1); }
export function main(): void {}
