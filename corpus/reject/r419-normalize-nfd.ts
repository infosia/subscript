// corpus: reject/r419-normalize-nfd
// purpose: normalize accepts only no argument or the literal "NFC"; NFD is not available.
// exercises: normalize, string
// questions: compiler.md §193 rule 3, compiler.md §193 rule 7
// tsc: accepts
// expected-error: S014, `normalize` is rejected: The form must be omitted or the literal "NFC"
export function main(): void {
  const s: string = "が";
  print(s.normalize("NFD"));
}
