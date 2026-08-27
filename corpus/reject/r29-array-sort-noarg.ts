// corpus: reject/r29-array-sort-noarg
// purpose: Rejects the no-argument `sort`: the lib's default sort
//          coerces elements to strings; a comparator is required (Q22).
// exercises: rejected-array-subset, array-methods
// questions: Q22
// tsc: accepts
// expected-error: sort requires a comparator
export function main(): void {
  const xs: i32[] = [3, 1, 2];
  xs.sort();
  print(xs.join(","));
}
