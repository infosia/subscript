// corpus: accept/a132-int-literal-64bit
// purpose: Reads integer literals from their spelling across the full u64 and i64 ranges.
// exercises: integer-literal-spelling, u64-range, i64-range, numeric-separators
// questions: §56, R26
// tsc: accepts; js-comparable: no C3: Full-width integers produce different output.
const decimalMax: u64 = 18446744073709551615;
const hexadecimalMax: u64 = 0xFFFFFFFFFFFFFFFF;
const separatedMax: u64 = 18_446_744_073_709_551_615;
const aboveF64Exact: u64 = 9007199254740993;
const signedMax: i64 = 9223372036854775807;
const signedMin: i64 = -9223372036854775808;

export function main(): void {
  print(`${decimalMax}`);
  print(`${hexadecimalMax}`);
  print(`${separatedMax}`);
  print(`${aboveF64Exact}`);
  print(`${signedMax}`);
  print(`${signedMin}`);
}
