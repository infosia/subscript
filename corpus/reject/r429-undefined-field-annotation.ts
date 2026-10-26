// corpus: reject/r429-undefined-field-annotation
// purpose: A T | undefined annotation on a field is rejected.
// exercises: undefined, annotation, field
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `T | undefined` is legal only as the result type of a function declaration or a class method
class Box {
  v: i32 = 1;
}
class Holder {
  item: Box | undefined = undefined;
}

export function main(): void {
  const h = new Holder();
  print(`${h.item === undefined}`);
}
