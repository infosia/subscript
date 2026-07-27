// corpus: reject/r42-map-iterator-member
// purpose: Rejects assigning a subject-only fused Map keys() view.
// exercises: for-of-subject-restriction, escaping-iterator-temporary
// questions: Q30
// expected-error: keys() is accepted only as a direct for-of subject

export function main(): void {
  const map: Map<i32, i32> = new Map<i32, i32>();
  const keys = map.keys();
}
