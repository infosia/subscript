// corpus: reject/r333-generic-inference-literal-conflict
// purpose: Rejects conflicting literal defaults in generic inference.
// exercises: generic-function, generic-inference
// expected-error: S100 at line 9
// questions: compiler.md §149
// tsc: accepts
function pair<T>(a: T, b: T): T { return a; }
export function main(): void {
  pair(1, 2.5);
}
