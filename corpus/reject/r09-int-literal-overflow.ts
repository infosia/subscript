// corpus: reject/r09-int-literal-overflow
// purpose: Rejects an integer literal outside its sized destination range.
// exercises: rejected-literal-overflow, i32-range
// questions: none
// expected-error: literal out of range for i32

const big: i32 = 3000000000;

export function main(): void {
  print(`${big}`);
}
