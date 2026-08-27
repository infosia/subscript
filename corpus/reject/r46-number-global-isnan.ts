// corpus: reject/r46-number-global-isnan
// purpose: Rejects the coercing global isNaN; Number.isNaN is accepted.
// exercises: rejected-number-coercion
// questions: Q25
// tsc: accepts
// expected-error: coercing global isNaN is rejected
export function main(): void {
  print(`${isNaN(1.0)}`);
}
