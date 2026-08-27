// corpus: reject/r32-array-splice
// purpose: Rejects `splice` insertion because variadic parameters are
//          the missing prerequisite; delete-only splice is accepted.
// exercises: rejected-array-variadic-form, array-methods
// questions: Q27
// tsc: accepts
// expected-error: variadic parameters are the missing prerequisite
export function main(): void {
  const xs: i32[] = [3, 1, 2];
  xs.splice(1, 2, 9, 9, 9);
  print(xs.join(","));
}
