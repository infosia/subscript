// corpus: accept/a74-p20-string-array-compound
// purpose: Keeps string-array element compound addition visible as a ship-C failure.
// exercises: string-array, index-write, compound-assignment, str-concat
// questions: none
// tsc: accepts
export function main(): void {
  const values: string[] = ["a"];
  const index: i32 = 0;
  values[index] += "s";
  print(values[index]);
}
