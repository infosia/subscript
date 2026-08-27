// corpus: reject/r80-regex-exec
// questions: Q31
// tsc: accepts
// purpose: Rejects exec because its result needs both an array-with-fields
//          shape and tuple typing, which the language does not have.
// exercises: RegExp.exec, array-with-fields, tuple-typing
// expected-error: S014 naming both missing type-system shapes
export function main(): void {
  const match = /x/.exec("x");
  print(`${match === null}`);
}
