// corpus: trap/t02-statements-after-fault
// purpose: Stops before statements following an array bounds fault.
// exercises: Array, indexing, sequential statements, trap unwind
// questions: none
// expected-trap: index-out-of-bounds at the out-of-range array read

export function main(): void {
  const values: i32[] = [7];
  print("before fault");
  const ignored: i32 = values[1];
  print(`after fault ${ignored}`);
}
