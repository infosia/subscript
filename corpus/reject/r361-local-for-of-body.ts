// corpus: reject/r361-local-for-of-body
// purpose: Assigns the local only in a for-of body.
// exercises: let, definite-assignment
// questions: compiler.md §158, collisions.md C24
// tsc: rejects TS2454
// expected-error: S100 at line 9, Rejects the unassigned binding.

function cond(): boolean { return true; }
export function main(): void { let x: i32; for (const i of [1, 2]) { x = i; } print(`${x}`); }
