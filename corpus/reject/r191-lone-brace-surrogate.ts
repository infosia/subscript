// corpus: reject/r191-lone-brace-surrogate
// purpose: Rejects a lone surrogate escape without a UTF-8 encoding.
// exercises: string-escape, lone-surrogate
// questions: Q5, §96
// tsc: accepts
// expected-error: S100 at the surrogate escape
export function main(): void {
  print("\u{D83D}");
}
