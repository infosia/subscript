// corpus: reject/r362-module-without-initializer
// purpose: Rejects module storage without an initializer.
// exercises: let, definite-assignment
// questions: compiler.md §158, collisions.md C24
// tsc: accepts
// expected-error: S100 at line 9, Rejects the unassigned binding.

function cond(): boolean { return true; }
let x: i32;
function assignLater(): void { x = 1; }
export function main(): void { print(`${x}`); }
