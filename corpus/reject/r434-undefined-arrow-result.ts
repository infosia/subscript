// corpus: reject/r434-undefined-arrow-result
// purpose: A T | undefined annotation on an arrow result is rejected.
// exercises: undefined, annotation, arrow
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `T | undefined` is legal only as the result type of a function declaration or a class method
class Box {
  v: i32 = 1;
}
export function main(): void {
  const pick = (): Box | undefined => undefined;
  print(`${pick() === undefined}`);
}
