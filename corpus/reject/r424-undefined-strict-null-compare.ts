// corpus: reject/r424-undefined-strict-null-compare
// purpose: A strict comparison of an undefined-spelled value with null is rejected; node gives false.
// exercises: undefined, equality
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `===` compares a `null` and an `undefined` absence; use `==`
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  print(`${pick() === null}`);
}
