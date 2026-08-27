// corpus: reject/r11-throw
// purpose: Rejects exception throwing.
// exercises: rejected-throw, exception
// questions: none
// tsc: accepts
// expected-error: exceptions are not in the language
function fail(): void {
  throw "failure";
}

export function main(): void {
  fail();
}
