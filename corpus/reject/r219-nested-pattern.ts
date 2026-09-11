// corpus: reject/r219-nested-pattern
// purpose: Rejects a binding pattern inside a binding pattern.
// exercises: binding-pattern, nested-pattern
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the recursive type checks a nested pattern needs
export function main(): void {
  const rows: i32[][] = [[1, 2]];
  const [[first, second]] = rows;
  print(`${first} ${second}`);
}
