// corpus: reject/r32-array-splice
// purpose: Rejects `splice`: structural mutation beyond the accepted
//          subset (push, pop, slice, fill, and the Q22 methods).
// exercises: rejected-array-subset, array-methods
// questions: Q22
// expected-error: splice is out of subset

export function main(): void {
  const xs: i32[] = [3, 1, 2];
  xs.splice(1, 1);
  print(xs.join(","));
}
