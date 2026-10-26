// corpus: reject/r430-undefined-parameter-annotation
// purpose: A T | undefined annotation on a parameter is rejected.
// exercises: undefined, annotation, parameter
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `T | undefined` is legal only as the result type of a function declaration or a class method
class Box {
  v: i32 = 1;
}
function absent(b: Box | undefined): boolean {
  return b === undefined;
}

export function main(): void {
  print(`${absent(new Box())}`);
}
