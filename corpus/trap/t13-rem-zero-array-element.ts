// corpus: trap/t13-rem-zero-array-element
// purpose: Traps on remainder in the second element of a dynamic-array literal.
// exercises: integer-remainder, array-literal, second-site-fault
// questions: none
// expected-trap: division-by-zero at the second array element

export function main(): void {
  const zero: i32 = 0;
  print("before array");
  const values: i32[] = [7, 84 % zero];
  print(`${values.length}`);
}
