// corpus: reject/r293-generic-member-write
// purpose: Rejects a member write on an unconstrained parameter.
// exercises: generic-body, type-parameter
// questions: compiler.md §143
// tsc: rejects TS2339
// expected-error: S018 at line 9, Rejects a member write on an unconstrained parameter.


function gf<T>(x: T): i32 { x.v = 3; return 0; }
export function main(): void {}
