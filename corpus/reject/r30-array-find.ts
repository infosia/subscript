// corpus: reject/r30-array-find
// purpose: Rejects `find`: a scalar `T[]` has no miss value (`T | null`
//          does not cover scalars); `findIndex` is the accepted
//          spelling (Q22).
// exercises: rejected-array-subset, array-methods
// questions: Q22
// tsc: accepts
// expected-error: find is out of subset; use findIndex
export function main(): void {
  const xs: i32[] = [3, 1, 2];
  const hit = xs.find((v: i32): boolean => v > 1);
  print(`${hit}`);
}
