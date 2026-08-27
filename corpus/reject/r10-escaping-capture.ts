// corpus: reject/r10-escaping-capture
// purpose: Rejects a capturing lambda that escapes its defining function.
// exercises: rejected-escaping-capture, capturing-lambda
// questions: none
// tsc: accepts
// expected-error: capturing lambdas may not escape
function makeAdder(offset: i32): (value: i32) => i32 {
  const local: i32 = offset;
  return (value: i32): i32 => value + local;
}

export function main(): void {
  const addFour: (value: i32) => i32 = makeAdder(4);
  print(`${addFour(6)}`);
}
