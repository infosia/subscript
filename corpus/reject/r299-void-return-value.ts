// corpus: reject/r299-void-return-value
// purpose: Rejects a value return from a void function.
// exercises: void, return
// questions: collisions.md C21
// tsc: accepts
// expected-error: S100 at line 9, Rejects a value return from a void function.

function g(x: void): void {
  return x;
}

export function main(): void {}
