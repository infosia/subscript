// corpus: reject/r150-parameter-and-local
// purpose: Rejects a body local that duplicates a function parameter.
// exercises: function-body-scope, duplicate-declaration
// questions: §67
// expected-error: S100 at the body-local declaration

function select(value: i32): i32 {
  const value: i32 = 7;
  return value;
}

export function main(): void {
  print(`${select(1)}`);
}
