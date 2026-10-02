// corpus: reject/r332-generic-inference-only-null
// purpose: Rejects a generic call without a unique candidate.
// exercises: generic-function, generic-inference
// expected-error: S100 at line 9
// questions: compiler.md §149
// tsc: accepts
function empty<T>(x: T | null): T | null { return x; }
export function main(): void {
  empty(null);
}
