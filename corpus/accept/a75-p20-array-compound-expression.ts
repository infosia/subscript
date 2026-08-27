// corpus: accept/a75-p20-array-compound-expression
// purpose: Keeps array compound assignment in expression position visible as a ship-C failure.
// exercises: Array, index-read, index-write, compound-assignment-expression
// questions: none
// tsc: accepts
export function main(): void {
  const values: i32[] = [10];
  const index: i32 = 0;
  const increment: i32 = 7;
  const result: i32 = (values[index] += increment);
  print(`${result},${values[index]}`);
}
