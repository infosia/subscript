// corpus: accept/a13-closures-noncapture
// purpose: Passes and calls function values that capture no surrounding state.
// exercises: function-value, noncapturing-function, indirect-call
// questions: Q1, Q10, Q12

function increment(value: i32): i32 {
  return value + 1;
}

function apply(operation: (value: i32) => i32, value: i32): i32 {
  return operation(value);
}

export function main(): void {
  const operation: (value: i32) => i32 = increment;
  print(`${apply(operation, 8)}`);
}
