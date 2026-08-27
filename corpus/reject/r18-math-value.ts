// corpus: reject/r18-math-value
// purpose: Rejects `Math` used as a value; it is a compiler namespace,
//          not an object.
// exercises: rejected-math-subset, math-intrinsics
// questions: Q19
// tsc: accepts
// expected-error: Math is a namespace; only member access is accepted
export function main(): void {
  const m = Math;
  print("unreachable");
}
