// corpus: reject/r85-invalid-regex-literal
// questions: Q31
// purpose: Rejects a malformed regex literal at check time.
// expected-error: S100 invalid regular-expression literal

export function main(): void {
  const regex: RegExp = /(/;
  print(regex.source);
}
