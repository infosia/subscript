// corpus: reject/r125-i64-literal-underflow
// purpose: Rejects an integer literal below the i64 range.
// exercises: rejected-literal-underflow, i64-range
// questions: none
// expected-error: literal out of range for i64

const low: i64 = -9223372036854775809;

export function main(): void {
  print(`${low}`);
}
