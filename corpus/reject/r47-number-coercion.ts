// corpus: reject/r47-number-coercion
// purpose: Rejects Number(x) coercion; explicit `as` is the conversion.
// exercises: rejected-number-coercion
// questions: Q25
// expected-error: Number(x) coercion is rejected

export function main(): void {
  const value: f64 = Number("1");
  print(`${value}`);
}
