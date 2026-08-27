// corpus: reject/r49-number-to-string-radix
// purpose: Rejects numeric toString without its required radix.
// exercises: required-number-formatting-argument
// questions: Q26
// tsc: accepts
// expected-error: toString requires an explicit radix
export function main(): void {
  const value: f64 = 255.0;
  print(value.toString());
}
