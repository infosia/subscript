// corpus: reject/r01-any
// purpose: Rejects any in a declaration.
// exercises: rejected-any
// questions: none
// tsc: accepts
// expected-error: any is not part of the language
const value: any = 1;

export function main(): void {
  print(`${value}`);
}
