// corpus: reject/r290-generic-relational-operand
// purpose: Rejects an unconstrained relational operand.
// exercises: generic-body, type-parameter
// questions: compiler.md §143
// tsc: rejects TS2365
// expected-error: S100 at line 10, Rejects an unconstrained relational operand.


class Box { v: i32 = 1; }
function gf<T>(x: T): boolean { return x > 1; }
export function main(): void { const a = gf<i32>(3); }
