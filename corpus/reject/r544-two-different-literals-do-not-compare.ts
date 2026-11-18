// corpus: reject/r544-two-different-literals-do-not-compare
// purpose: Equality rejects distinct literal operands of the same kind.
// exercises: const, equality, readonly, literal
// questions: compiler.md §330 rules 1 and 2
// tsc: rejects TS2367
// expected-error: S100 at line 15
const MODE = 1;
const OTHER = 2;
const NEGATIVE = -1;
const A = 1.5;
const F = true;
const S = "a";
class K { readonly size = 4; static readonly Z = 0; }
export function main(): void {
  MODE === 2;
  MODE !== 2;
  MODE == 2;
  MODE != 2;
  NEGATIVE === 1;
  MODE === OTHER;
  A === 2.5;
  F === false;
  S === "b";
  1 === 2;
  const k = new K();
  k.size === 5;
  K.Z === 1;
  const z = K.Z;
  z === 1;
  const x = 3;
  x === 4;
}
