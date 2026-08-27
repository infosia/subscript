// corpus: reject/r38-map-f16-key
// purpose: Rejects storage-only f16 as a Map key.
// exercises: map-key-whitelist, f16
// questions: Q24, Q23
// tsc: accepts
// expected-error: `f16` is not a permitted Map/Set key kind (Q24)
export function main(): void {
  const map: Map<f16, i32> = new Map<f16, i32>();
  print(`${map.size}`);
}
