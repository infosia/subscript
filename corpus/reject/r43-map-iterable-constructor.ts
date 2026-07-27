// corpus: reject/r43-map-iterable-constructor
// purpose: Rejects construction from an iterable.
// exercises: map-api-subset, missing-tuple-type
// questions: Q30
// expected-error: iterable Map construction needs a tuple type

export function main(): void {
  const map: Map<i32, i32> = new Map<i32, i32>([[1, 2]]);
  print(`${map.size}`);
}
