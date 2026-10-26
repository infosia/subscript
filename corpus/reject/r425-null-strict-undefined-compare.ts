// corpus: reject/r425-null-strict-undefined-compare
// purpose: A strict comparison of a null-spelled value with undefined is rejected; node gives false.
// exercises: undefined, equality
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `===` compares a `null` and an `undefined` absence; use `==`
class Box {
  v: i32 = 1;
}
export function main(): void {
  const n: Box | null = null;
  print(`${n === undefined}`);
}
