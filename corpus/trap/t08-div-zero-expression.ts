// corpus: trap/t08-div-zero-expression
// purpose: Traps on integer division by zero in expression position.
// exercises: integer-division, expression, division-by-zero
// questions: none
// expected-trap: division-by-zero at the division expression

export function main(): void {
  const zero: i32 = 0;
  print("before division");
  const quotient: i32 = 84 / zero;
  print(`${quotient}`);
}
