// corpus: reject/r151-duplicate-const
// purpose: Rejects two const declarations of one name in one block.
// exercises: block-scope, duplicate-declaration
// questions: §67
// tsc: rejects TS2451
// expected-error: S017 at the second declaration
export function main(): void {
  const value: i32 = 1;
  const value: i32 = 2;
  print(`${value}`);
}
