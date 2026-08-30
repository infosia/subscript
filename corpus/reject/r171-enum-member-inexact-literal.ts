// corpus: reject/r171-enum-member-inexact-literal
// purpose: Rejects an enum member literal that is not exact in f64 and is outside i32.
// exercises: numeric-enum, integer-literal-spelling
// questions: Q3
// tsc: accepts
// expected-error: S100 at the enum member initializer
enum Huge {
  A = 9007199254740993,
}
export function main(): void {
  print(`${Huge.A as i32}`);
}
