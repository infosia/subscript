// corpus: reject/r170-enum-member-out-of-range
// purpose: Rejects an enum member value outside the i32 range.
// exercises: numeric-enum, integer-literal-range
// questions: Q3
// tsc: accepts
// expected-error: S100 at the enum member initializer
enum Wide {
  A = 1,
  B = 2147483648,
}
export function main(): void {
  print(`${Wide.A as i32}`);
}
