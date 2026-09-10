// corpus: reject/r206-array-from-bare-map
// purpose: Rejects a bare Map as an Array.from source.
// exercises: array-from, map-iteration, invariant-5
// questions: Q22, compiler section 105
// tsc: accepts
// expected-error: S014 naming the pair TypeScript binds
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  const keys = Array.from(map);
  print(`${keys.length}`);
}
