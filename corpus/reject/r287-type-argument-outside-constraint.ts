// corpus: reject/r287-type-argument-outside-constraint
// purpose: Rejects a type argument that does not satisfy the constraint of its type parameter.
// exercises: generic-function, type-parameter, type-parameter-constraint
// questions: compiler.md §135
// tsc: rejects TS2344
// expected-error: S100 at line 12, the type argument string outside the constraint i32 of T
function keep<T extends i32>(value: T): T {
  return value;
}

export function main(): void {
  print(keep<string>("a"));
}
