// corpus: accept/a14-closures-capture
// purpose: Provides the single minimal probe for a capturing lambda.
// exercises: capturing-lambda, closure-environment, indirect-call
// questions: Q1, Q10, Q12

export function main(): void {
  const offset: i32 = 5;
  const addOffset: (value: i32) => i32 = (value: i32): i32 => value + offset;
  print(`${addOffset(7)}`);
}
