// corpus: reject/r28-string-tolocaleupper
// purpose: Rejects `toLocaleUpperCase`: locale-dependent case mapping is
//          out of the accepted subset (Q21 accepts only locale-insensitive
//          Unicode Default Case Conversion).
// exercises: rejected-string-subset, string-methods
// questions: Q21
// expected-error: toLocaleUpperCase is locale-dependent; use toUpperCase

export function main(): void {
  const s: string = "hi";
  const t: string = s.toLocaleUpperCase();
  print(t);
}
