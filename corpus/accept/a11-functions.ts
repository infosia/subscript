// corpus: accept/a11-functions
// purpose: Calls plain functions with explicit and default parameter values.
// exercises: plain-function, default-parameter, return-value
// questions: Q1, Q12
// tsc: accepts; js-comparable: yes
function multiply(left: i32, right: i32): i32 {
  return left * right;
}

function scale(value: i32, factor: i32 = 3): i32 {
  return multiply(value, factor);
}

export function main(): void {
  const defaulted: i32 = scale(7);
  const explicit: i32 = scale(7, 4);
  print(`${defaulted},${explicit}`);
}
