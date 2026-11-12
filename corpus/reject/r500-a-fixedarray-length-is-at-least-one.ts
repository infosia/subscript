// corpus: reject/r500-a-fixedarray-length-is-at-least-one
// purpose: A FixedArray length is at least 1, because C has no array with the length zero.
// exercises: FixedArray, type-annotation
// questions: compiler.md §254.1 rule 1
// tsc: accepts
// expected-error: S100 at line 8, a `FixedArray` length is at least 1

function mk(): FixedArray<i32, 0> {
  return [];
}

export function main(): void {
  const a = mk();
  print(`a ${a.length}`);
}
