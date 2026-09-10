// corpus: reject/r211-array-of-variadic
// purpose: Rejects Array.of at variable arity.
// exercises: array-namespace, variadic-arguments
// questions: Q22, compiler section 105
// tsc: accepts
// expected-error: S014 naming the variadic parameter prerequisite
export function main(): void {
  const xs: i32[] = Array.of<i32>(1, 2);
  print(`${xs.length}`);
}
