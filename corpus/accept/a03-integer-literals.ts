// corpus: accept/a03-integer-literals
// purpose: Places suffix-less integer literals into several sized numeric contexts.
// exercises: numeric-literals, typed-initializer, typed-argument, typed-array
// questions: Q1, Q4, Q12
// tsc: accepts; js-comparable: yes
function addOffset(value: i32, offset: i32): i32 {
  return value + offset;
}

export function main(): void {
  const initialized: i32 = 7;
  const values: i32[] = [2, 3, 5];
  const result: i32 = addOffset(initialized, 11) + values[0] + values[1] + values[2];
  print(`${result}`);
}
