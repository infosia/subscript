// corpus: reject/r43-map-iterable-constructor
// purpose: Rejects construction from an iterable.
// exercises: map-api-subset, iterator-protocol
// questions: Q24
// expected-error: iterable Map construction is rejected (Q24)

export function main(): void {
  const map: Map<i32, i32> = new Map<i32, i32>([[1, 2]]);
  print(`${map.size}`);
}
