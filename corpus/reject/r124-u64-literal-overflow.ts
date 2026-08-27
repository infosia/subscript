// corpus: reject/r124-u64-literal-overflow
// purpose: Rejects an integer literal above the u64 range.
// exercises: rejected-literal-overflow, u64-range
// questions: none
// tsc: accepts
// expected-error: literal out of range for u64
const big: u64 = 18446744073709551616;

export function main(): void {
  print(`${big}`);
}
