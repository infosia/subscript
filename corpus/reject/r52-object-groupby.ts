// corpus: reject/r52-object-groupby
// purpose: Rejects Object.groupBy because its null-prototype object
//          result has no language type.
// exercises: Object.groupBy, null-prototype-object, rejected-standard-library
// expected-error: S014 at the groupBy member
// questions: Q27
// tsc: rejects TS2550
export function main(): void {
  Object.groupBy([1, 2], (value: i32): string => `${value}`);
}
