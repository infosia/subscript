// corpus: reject/r49-number-to-string-radix
// purpose: Rejects numeric toString(radix); Q14 templates spell base 10.
// exercises: rejected-number-formatting
// questions: Q14, Q25
// expected-error: toString(radix) is rejected

export function main(): void {
  const value: f64 = 255.0;
  print(value.toString(16));
}
