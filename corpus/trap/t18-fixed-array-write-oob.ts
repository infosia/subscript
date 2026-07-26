// corpus: trap/t18-fixed-array-write-oob
// purpose: Traps before an unproven out-of-range write to a FixedArray.
// exercises: FixedArray, index-write, index-out-of-bounds
// questions: Q3
// expected-trap: index-out-of-bounds at the FixedArray write

function write(values: FixedArray<i32, 2>, index: i32): void {
  values[index] = 9;
}

export function main(): void {
  const values: FixedArray<i32, 2> = [7, 8];
  print("before write");
  write(values, 4);
  print("after write");
}
