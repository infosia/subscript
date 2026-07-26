// corpus: trap/t12-div-zero-array-element
// purpose: Traps on division in the second element of a dynamic-array literal.
// exercises: integer-division, array-literal, second-site-fault
// questions: none
// expected-trap: division-by-zero at the second array element

export function main(): void {
  const zero: i32 = 0;
  print("before array");
  const values: i32[] = [7, 84 / zero];
  print(`${values.length}`);
}
