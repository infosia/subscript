// corpus: reject/r433-undefined-exported-result
// purpose: An exported entry function with a T | undefined result is rejected; C has one null pointer.
// exercises: undefined, boundary
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, a function that crosses the C boundary has no `T | undefined`
class Box {
  v: i32 = 1;
}
export function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  print(`${pick() === undefined}`);
}
