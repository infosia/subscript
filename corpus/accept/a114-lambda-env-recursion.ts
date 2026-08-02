// corpus: accept/a114-lambda-env-recursion
// purpose: Pins a capturing lambda's environment across recursive re-entry of its defining function.
// exercises: capturing-lambda, recursion, automatic-environment-storage
// questions: Q35

function capturedAfterRecursion(value: i32): i32 {
  const captured: i32 = value;
  const read: () => i32 = (): i32 => captured;
  if (value > 0) {
    capturedAfterRecursion(value - 1);
  }
  return read();
}

export function main(): void {
  print(`${capturedAfterRecursion(3)}`);
}
