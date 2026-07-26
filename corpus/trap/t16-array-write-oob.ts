// corpus: trap/t16-array-write-oob
// purpose: Traps before an out-of-range write to a dynamic array.
// exercises: Array, index-write, index-out-of-bounds
// questions: none
// expected-trap: index-out-of-bounds at the dynamic-array write

export function main(): void {
  const values: i32[] = [7];
  const index: i32 = 4;
  print("before write");
  values[index] = 9;
  print("after write");
}
