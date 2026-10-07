// corpus: reject/r391-nested-nominal-classes
// purpose: Pin the fact split of compiler section 173.
// exercises: rejection-class, divergence-block
// questions: compiler.md §173
// tsc: accepts
// expected-error: S100: type mismatch: the initializer expects `P[]`, got `Q[]`

class P { x: i32 = 1; }
class Q { x: i32 = 1; }
function q(): Q { return new Q(); }
function take(v: Q): i32 { return v.x; }
export function main(): void { const qs: Q[] = [new Q()]; const ps: P[] = qs; const p: () => P = q; const f: (v: P) => i32 = take; }
