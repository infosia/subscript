// corpus: reject/r212-new-array-length
// purpose: Rejects the length constructor of Array.
// exercises: array-namespace, array-holes
// questions: Q22, compiler section 105
// tsc: accepts
// expected-error: S014 naming the missing array hole
export function main(): void {
  const xs: i32[] = new Array<i32>(3);
  print(`${xs.length}`);
}
