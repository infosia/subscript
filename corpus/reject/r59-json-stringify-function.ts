// corpus: reject/r59-json-stringify-function
// purpose: Rejects a function-typed JSON.stringify input.
// exercises: JSON.stringify, function-value, rejected-input-family
// expected: S014 at stringify
// questions: Q28, C5

function identity(value: i32): i32 {
  return value;
}

export function main(): void {
  const value: (value: i32) => i32 = identity;
  JSON.stringify(value);
}
