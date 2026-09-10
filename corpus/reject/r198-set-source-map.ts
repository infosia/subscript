// corpus: reject/r198-set-source-map
// purpose: Rejects a Map as the source of a Set construction.
// exercises: set-source-construction, missing-tuple-type
// questions: Q30, compiler section 103
// tsc: rejects TS2769
// expected-error: S014 naming invariant 5 and TS2769
export function main(): void {
  const source: Map<i32, string> = new Map<i32, string>();
  const values: Set<i32> = new Set<i32>(source);
  print(`${values.size}`);
}
