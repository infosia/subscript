// corpus: reject/r297-void-binding
// purpose: Rejects a binding of a void return value.
// exercises: void, binding
// questions: collisions.md C21
// tsc: accepts
// expected-error: S100 at line 11, Rejects a binding of a void return value.

function f(): void {}

export function main(): void {
  const a = f();
}
