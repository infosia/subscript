// corpus: reject/r359-local-one-branch
// purpose: Assigns the local in only one branch.
// exercises: let, definite-assignment
// questions: compiler.md §158, collisions.md C24
// tsc: rejects TS2454
// expected-error: S100 at line 9, Rejects the unassigned binding.

function cond(): boolean { return true; }
export function main(): void { let x: i32; if (cond()) { x = 1; } print(`${x}`); }
