// corpus: reject/r42-map-iterator-member
// purpose: Rejects Map iterator-protocol members; traversal is forEach.
// exercises: map-api-subset, iterator-protocol
// questions: Q24
// expected-error: keys requires the iterator protocol (Q24)

export function main(): void {
  const map: Map<i32, i32> = new Map<i32, i32>();
  map.keys();
}
