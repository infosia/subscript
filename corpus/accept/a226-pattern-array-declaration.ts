// corpus: accept/a226-pattern-array-declaration
// purpose: Binds array elements by position in a const and a let declaration.
// observable: a skipped element binds no name, a skipped position past the end does not trap, and the source evaluates one time.
// exercises: binding-pattern, array-element, skipped-element, empty-pattern
// questions: Q30, compiler section 107
// tsc: accepts; js-comparable: yes
function made(): i32[] {
  print("made");
  return [10, 20, 30];
}

export function main(): void {
  const [first, second] = made();
  print(`${first} ${second}`);
  const [, middle] = made();
  print(`${middle}`);
  const [] = made();
  let [head, tail] = made();
  head = head + 1;
  tail = tail + 1;
  print(`${head} ${tail}`);
  const fixed: FixedArray<i32, 3> = [4, 5, 6];
  const [low, , high] = fixed;
  print(`${low} ${high}`);
  const short: i32[] = [7];
  const [only, ,] = short;
  print(`${only}`);
}
