// corpus: accept/a19-modules/math
// purpose: Exports a typed function from the secondary module.
// exercises: module-export, cross-file-function
// questions: Q1

export function triangular(value: i32): i32 {
  return (value * (value + 1)) / 2;
}
