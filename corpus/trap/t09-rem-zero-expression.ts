// corpus: trap/t09-rem-zero-expression
// purpose: Traps on integer remainder by zero in expression position.
// exercises: integer-remainder, expression, division-by-zero
// questions: none
// expected-trap: division-by-zero at the remainder expression

export function main(): void {
  const zero: i32 = 0;
  print("before remainder");
  const remainder: i32 = 84 % zero;
  print(`${remainder}`);
}
