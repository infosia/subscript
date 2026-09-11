// corpus: reject/r220-assignment-pattern
// purpose: Rejects a destructuring assignment to existing names.
// exercises: binding-pattern, assignment-target
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the missing write order
export function main(): void {
  const values: i32[] = [1, 2];
  let first: i32 = 0;
  let second: i32 = 0;
  [first, second] = values;
  print(`${first} ${second}`);
}
