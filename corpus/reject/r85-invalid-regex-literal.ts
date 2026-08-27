// corpus: reject/r85-invalid-regex-literal
// questions: Q31
// tsc: rejects TS1005
// purpose: Rejects a malformed regex literal at check time.
// exercises: RegExp-literal, invalid-pattern, check-time-rejection
// expected-error: S100 invalid regular-expression literal
export function main(): void {
  const regex: RegExp = /(/;
  print(regex.source);
}
