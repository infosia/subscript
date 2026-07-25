// corpus: reject/r50-parse-int-no-radix
// purpose: Rejects parseInt without the required explicit radix.
// exercises: rejected-parse-arity
// questions: Q25
// expected-error: parseInt requires an explicit radix

export function main(): void {
  const value: f64 = parseInt("10");
  print(`${value}`);
}
