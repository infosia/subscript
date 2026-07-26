// corpus: trap/t03-loop-stops-at-fault
// purpose: Stops a loop and its enclosing function at an array bounds fault.
// exercises: Array, indexing, while, post-fault loop body, post-loop statement
// questions: none
// expected-trap: index-out-of-bounds inside the second loop iteration

export function main(): void {
  const values: i32[] = [7];
  let i: i32 = 0;
  while (i < 3) {
    print(`iteration ${i}`);
    if (i === 1) {
      const ignored: i32 = values[1];
      print(`after fault ${ignored}`);
    }
    print(`body end ${i}`);
    i += 1;
  }
  print("after loop");
}
