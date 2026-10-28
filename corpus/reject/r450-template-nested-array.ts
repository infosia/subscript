// corpus: reject/r450-template-nested-array
// purpose: An array in a template is its join, so an element that join cannot format is rejected with the join text.
// exercises: template-literal, array-join
// questions: compiler.md §204.1 rule 2
// tsc: accepts
// expected-error: S014, `join` formats elements by the Q14 interpolation rules
export function main(): void {
  const nested: i32[][] = [[1, 2], [3]];
  print(`${nested}`);
}
