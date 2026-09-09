// corpus: reject/r190-lone-surrogate-before-character
// purpose: Rejects a lone surrogate escape without a UTF-8 encoding.
// exercises: string-escape, lone-surrogate
// questions: Q5, §96
// tsc: accepts
// expected-error: S100 at the surrogate escape
export function main(): void {
  print("a\ud83db");
}
