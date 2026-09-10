// corpus: reject/r200-map-source-map
// purpose: Rejects a Map source for a Map construction.
// exercises: map-api-subset, missing-tuple-type
// questions: Q30, compiler section 103
// tsc: accepts
// expected-error: S014 naming the missing tuple type
export function main(): void {
  const source: Map<i32, i32> = new Map<i32, i32>();
  const copy: Map<i32, i32> = new Map<i32, i32>(source);
  print(`${copy.size}`);
}
