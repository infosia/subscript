// corpus: reject/r210-array-is-array
// purpose: Rejects Array.isArray.
// exercises: array-namespace, runtime-classification
// questions: Q22, compiler section 105
// tsc: accepts
// expected-error: S014 naming the static answer and the open runtime test
export function main(): void {
  const xs: i32[] = [1, 2];
  const flag: boolean = Array.isArray(xs);
  print(`${flag}`);
}
