// corpus: reject/r78-call-spread-variadic
// purpose: Rejects call spread for its missing prerequisite.
// exercises: call-spread, variadic-parameters
// questions: Q30
// tsc: rejects TS2556
// expected-error: call spread requires variadic parameters
function pair(left: i32, right: i32): void {
  print(`${left}:${right}`);
}

export function main(): void {
  const values: i32[] = [1, 2];
  pair(...values);
}
