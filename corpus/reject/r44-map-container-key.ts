// corpus: reject/r44-map-container-key
// purpose: Rejects a Map/Set handle key consistently with a dynamic-array handle.
// exercises: map-key-whitelist, container-handle
// questions: Q24, Q22
// tsc: accepts
// expected-error: `Map<i32, i32>` is not a permitted Map/Set key kind (Q24)
export function main(): void {
  const map: Map<Map<i32, i32>, i32> = new Map<Map<i32, i32>, i32>();
  print(`${map.size}`);
}
