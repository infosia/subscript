// corpus: trap/t17-fixed-array-read-oob
// purpose: Traps on an unproven out-of-range read from a FixedArray.
// exercises: FixedArray, index-read, index-out-of-bounds
// questions: Q3
// expected-trap: index-out-of-bounds at the FixedArray read

function read(values: FixedArray<i32, 2>, index: i32): i32 {
  return values[index];
}

export function main(): void {
  const values: FixedArray<i32, 2> = [7, 8];
  print("before read");
  print(`${read(values, 4)}`);
}
