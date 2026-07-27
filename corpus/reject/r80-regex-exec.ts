// corpus: reject/r80-regex-exec
// questions: Q31
// purpose: Rejects exec because its result needs both an array-with-fields
//          shape and tuple typing, which the language does not have.
// expected-error: S014 naming both missing type-system shapes

export function main(): void {
  const match = /x/.exec("x");
  print(`${match === null}`);
}
