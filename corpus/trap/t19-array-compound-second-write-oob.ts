// corpus: trap/t19-array-compound-second-write-oob
// purpose: Faults at a compound assignment's second dynamic-array resolution.
// exercises: Array, compound-assignment, read-then-write, second-site-fault
// questions: none
// expected-trap: index-out-of-bounds at the post-RHS write resolution

function shrink(values: i32[]): i32 {
  values.pop();
  return 5;
}

export function main(): void {
  const values: i32[] = [10, 20];
  const index: i32 = 1;
  print("before compound");
  values[index] += shrink(values);
  print("after compound");
}
