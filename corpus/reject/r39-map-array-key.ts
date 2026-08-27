// corpus: reject/r39-map-array-key
// purpose: Rejects a dynamic array key because Q24 defines no array hash.
// exercises: map-key-whitelist, dynamic-array
// questions: Q24, Q22
// tsc: accepts
// expected-error: `i32[]` is not a permitted Map/Set key kind (Q24)
export function main(): void {
  const map: Map<i32[], i32> = new Map<i32[], i32>();
  print(`${map.size}`);
}
