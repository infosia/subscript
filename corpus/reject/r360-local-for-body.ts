// corpus: reject/r360-local-for-body
// purpose: Assigns the local only in a for body.
// exercises: let, definite-assignment
// questions: compiler.md §158, collisions.md C24
// tsc: rejects TS2454
// expected-error: S100 at line 9, Rejects the unassigned binding.

function cond(): boolean { return true; }
export function main(): void { let x: i32; for (let i: i32 = 0; i < 2; i++) { x = i; } print(`${x}`); }
