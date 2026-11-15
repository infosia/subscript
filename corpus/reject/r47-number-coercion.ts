// corpus: reject/r47-number-coercion
// purpose: Rejects Number("1") coercion; parseFloat("1") converts the string explicitly.
// exercises: rejected-number-coercion
// questions: Q25
// tsc: accepts
// expected-error: Number(x) coercion is rejected
export function main(): void {
  const value: f64 = Number("1");
  print(`${value}`);
}
