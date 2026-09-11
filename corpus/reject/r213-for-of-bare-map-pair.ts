// corpus: reject/r213-for-of-bare-map-pair
// purpose: Rejects a pair pattern over a bare Map for-of subject.
// exercises: binding-pattern, for-of, map-iteration
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S014 naming the pair TypeScript binds
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  for (const [key, value] of map) {
    print(`${key} ${value}`);
  }
}
