// corpus: reject/r236-sandbox-source-depth
// profile: sandbox
// purpose: Rejects a parenthesis nest over the profile's nesting limit of 256.
// exercises: sandbox-profile, nesting-depth, parentheses
// questions: Q6
// tsc: accepts
// expected-error: S026 where the nesting depth reaches 257, at the innermost parenthesis
export function main(): void {
  const value: i32 = (((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((7)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
  print(`${value}`);
}
