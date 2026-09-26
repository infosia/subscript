// corpus: reject/r239-stringify-error
// purpose: Rejects an Error as JSON.stringify input, because the Error classes are not JSON types.
// exercises: JSON.stringify, error-class, rejected-input-family, exception
// expected-error: S014 at stringify
// questions: Q9, Q28
// tsc: accepts
export function main(): void {
  const failure: Error = new Error("not a JSON value");
  print(JSON.stringify(failure));
}
