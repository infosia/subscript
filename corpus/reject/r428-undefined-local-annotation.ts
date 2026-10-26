// corpus: reject/r428-undefined-local-annotation
// purpose: A T | undefined annotation on a local is rejected.
// exercises: undefined, annotation
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `T | undefined` is legal only as the result type of a function declaration or a class method
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  const b: Box | undefined = pick();
  print(`${b === undefined}`);
}
