// corpus: reject/r55-array-callback-container
// purpose: Rejects the callback's reference to the array being iterated.
// exercises: Array-callback, container-parameter, non-escaping-by-construction
// reason: f(v, i) passes a value and integer, while f(v, i, arr) passes
//         the container reference and violates C5 non-escaping-by-construction.
// expected: S014 naming C5 at the three-parameter callback
// questions: Q27, C5

export function main(): void {
  const values: i32[] = [1, 2, 3];
  values.map((value: i32, index: i32, array: i32[]): i32 => value + index + array.length);
}
