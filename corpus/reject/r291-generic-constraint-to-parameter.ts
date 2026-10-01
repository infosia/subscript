// corpus: reject/r291-generic-constraint-to-parameter
// purpose: Rejects a constraint value assigned to its parameter.
// exercises: generic-body, type-parameter
// questions: compiler.md §143
// tsc: rejects TS2322
// expected-error: S100 at line 10, Rejects a constraint value assigned to its parameter.


class Box { v: i32 = 1; }
function gf<T extends Box>(b: Box): i32 { const t: T = b; return t.v; }
export function main(): void { const a = gf<Box>(new Box()); }
