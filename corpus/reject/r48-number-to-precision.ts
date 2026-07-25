// corpus: reject/r48-number-to-precision
// purpose: Rejects toPrecision outside the accepted Number subset.
// exercises: rejected-number-formatting
// questions: Q25
// expected-error: toPrecision is outside the accepted subset

export function main(): void {
  const value: f64 = 1.25;
  print(value.toPrecision(2));
}
