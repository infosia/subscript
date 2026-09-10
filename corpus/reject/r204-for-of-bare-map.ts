// corpus: reject/r204-for-of-bare-map
// purpose: Rejects a bare Map as a for-of subject.
// exercises: for-of, map-iteration, invariant-5
// questions: Q30, compiler section 104
// tsc: accepts
// expected-error: S014 naming the pair TypeScript binds
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  for (const key of map) {
    print(`${key}`);
  }
}
