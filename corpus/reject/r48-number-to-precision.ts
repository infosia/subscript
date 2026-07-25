// corpus: reject/r48-number-to-precision
// purpose: Rejects toPrecision without its required digit count.
// exercises: required-number-formatting-argument
// questions: Q26
// expected-error: toPrecision requires an explicit digit count

export function main(): void {
  const value: f64 = 1.25;
  print(value.toPrecision());
}
