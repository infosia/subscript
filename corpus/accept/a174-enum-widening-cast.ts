// corpus: accept/a174-enum-widening-cast
// purpose: An enum value converts to i64 and to i32 with the member's value.
// observable: Both conversions print the declared member values, on every tier.
// exercises: numeric-enum, enum-to-integer-cast, widening-cast
// questions: Q3
// tsc: accepts; js-comparable: no Q3: `as i64` has no JavaScript width.
enum Small {
  A = 7,
  B = 2147483647,
  C = -2147483648,
}
export function main(): void {
  const v: i64 = Small.A as i64;
  const w: i64 = Small.B as i64;
  const x: i64 = Small.C as i64;
  const u: i32 = Small.B as i32;
  print(`${v} ${w} ${x} ${u}`);
  print(`${Small.A as i64} ${Small.C as i64}`);
}
