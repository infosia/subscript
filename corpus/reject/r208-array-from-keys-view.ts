// corpus: reject/r208-array-from-keys-view
// purpose: Rejects a keys() view as an Array.from source.
// exercises: array-from, map-keys, fused-view
// questions: Q22, Q30, compiler section 105
// tsc: accepts
// expected-error: S014 naming the direct for-of subject rule
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  const keys = Array.from(map.keys());
  print(`${keys.length}`);
}
