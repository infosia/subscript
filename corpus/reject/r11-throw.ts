// corpus: reject/r11-throw
// purpose: Rejects a `throw` whose operand is not an Error-family object.
// exercises: rejected-throw, exception
// questions: none
// tsc: accepts
// expected-error: S010 at the `throw` of a string operand
function fail(): void {
  throw "failure";
}

export function main(): void {
  fail();
}
