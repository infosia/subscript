// corpus: reject/r420-normalize-non-literal
// purpose: A normalize argument that is not the literal "NFC" is rejected, even when its value is "NFC".
// exercises: normalize, string
// questions: compiler.md §193 rule 3
// tsc: accepts
// expected-error: S014, `normalize` is rejected: The form must be omitted or the literal "NFC"
export function main(): void {
  const form: string = "NFC";
  const s: string = "が";
  print(s.normalize(form));
}
