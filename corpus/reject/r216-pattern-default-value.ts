// corpus: reject/r216-pattern-default-value
// purpose: Rejects a default value inside a binding pattern.
// exercises: binding-pattern, default-value, missing-undefined
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the missing rule for an absent element
export function main(): void {
  const values: i32[] = [];
  const [first = 1] = values;
  print(`${first}`);
}
