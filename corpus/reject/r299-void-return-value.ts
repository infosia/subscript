// corpus: reject/r299-void-return-value
// purpose: Rejects a void parameter and its use as a return operand.
// exercises: void, return
// questions: collisions.md C21
// tsc: accepts
// expected-error: S100 at line 8, Rejects a void parameter and its use as a return operand.

function g(x: void): void {
  return x;
}

export function main(): void {}
