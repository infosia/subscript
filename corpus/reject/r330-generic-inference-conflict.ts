// corpus: reject/r330-generic-inference-conflict
// purpose: Rejects a generic call without a unique candidate.
// exercises: generic-function, generic-inference
// expected-error: S100 at line 11
// questions: compiler.md §149
// tsc: accepts
function pair<T>(a: T, b: T): T { return a; }
export function main(): void {
  const n: i32 = 1;
  const f: f64 = 2.5;
  pair(n, f);
}
