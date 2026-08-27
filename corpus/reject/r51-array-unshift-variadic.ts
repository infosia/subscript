// corpus: reject/r51-array-unshift-variadic
// purpose: Rejects multi-element `unshift` because variadic parameters
//          are the missing prerequisite; one-element unshift is accepted.
// exercises: rejected-array-variadic-form, array-methods
// questions: Q27
// tsc: accepts
// expected-error: variadic parameters are the missing prerequisite
export function main(): void {
  const xs: i32[] = [1];
  xs.unshift(-1, 0);
  print(xs.join(","));
}
