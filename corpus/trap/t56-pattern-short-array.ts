// corpus: trap/t56-pattern-short-array
// purpose: An absent required element in an array binding pattern takes the ordinary checked read.
// exercises: binding-pattern, array-element, index-out-of-bounds, trap unwind
// questions: Q30, compiler section 107
// expected-trap: index-out-of-bounds at the read of the absent element
export function main(): void {
  const values: i32[] = [7];
  const [present] = values;
  print(`${present}`);
  const [first, second] = values;
  print(`${first} ${second}`);
}
