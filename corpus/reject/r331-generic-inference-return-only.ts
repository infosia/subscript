// corpus: reject/r331-generic-inference-return-only
// purpose: Rejects a generic call without a unique candidate.
// exercises: generic-function, generic-inference
// expected-error: S100 at line 9
// questions: compiler.md §149
// tsc: accepts
function empty<T>(): T | null { return null; }
export function main(): void {
  empty();
}
