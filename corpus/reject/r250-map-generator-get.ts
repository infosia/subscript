// corpus: reject/r250-map-generator-get
// purpose: Rejects get on a Map whose Generator value type has no nullable form.
// exercises: map-get-miss, generator-value
// questions: Q24, C7
// tsc: accepts
// expected-error: S014: get(key) is rejected: The value type has no `| null` form of the map's value representation; use getOr (Q24)
export function main(): void {
  const map: Map<i32, Generator<i32>> = new Map<i32, Generator<i32>>();
  map.get(1);
}
