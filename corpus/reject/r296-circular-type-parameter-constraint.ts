// corpus: reject/r296-circular-type-parameter-constraint
// purpose: Rejects a cycle of direct type-parameter constraints.
// exercises: generic-body, type-parameter, constraint-cycle
// questions: compiler.md §143
// tsc: rejects TS2313
// expected-error: S100 at line 8, The type parameter has a circular constraint.

function gf<T extends U, U extends T>(x: T): i32 { return 0; }
export function main(): void {}
