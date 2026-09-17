// corpus: reject/r239-sandbox-nesting-unary
// profile: sandbox
// purpose: Rejects a prefix-operator chain past the profile's nesting limit; no bracket token counts an operator.
// exercises: sandbox-profile, nesting-depth, unary-operator
// questions: Q6
// tsc: accepts
// expected-error: S026 where the expression nesting reaches 257
export function main(): void {
  const flag: boolean = !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!true;
  print(`${flag}`);
}
