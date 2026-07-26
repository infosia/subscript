// corpus: trap/t11-rem-zero-loop-condition
// purpose: Traps on integer remainder by zero while evaluating a loop condition.
// exercises: integer-remainder, while-condition, division-by-zero
// questions: none
// expected-trap: division-by-zero in the while condition

export function main(): void {
  const zero: i32 = 0;
  print("before loop");
  while ((12 % zero) === 0) {
    print("loop body");
  }
  print("after loop");
}
