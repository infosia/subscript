// corpus: reject/r236-sandbox-source-depth
// profile: sandbox
// purpose: Rejects a bracket depth over 256 under the sandbox profile, before the parser runs.
// exercises: sandbox-profile, source-limit, bracket-depth
// questions: Q6
// tsc: accepts
// expected-error: S026 where the bracket depth reaches 257, at the 256th parenthesis
export function main(): void {
  const value: i32 = (((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((7)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
  print(`${value}`);
}
