// corpus: reject/r217-pattern-array-rest
// purpose: Rejects a rest element in an array binding pattern.
// exercises: binding-pattern, rest-element, array-allocation
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the rest array allocation
export function main(): void {
  const values: i32[] = [1, 2, 3];
  const [head, ...rest] = values;
  print(`${head} ${rest.length}`);
}
