// corpus: reject/r196-high-surrogate-before-brace-character
// purpose: Rejects a high surrogate before a brace escape with a non-surrogate value.
// exercises: string-escape, lone-surrogate
// questions: Q5, §96
// tsc: accepts
// expected-error: S100 at the first surrogate escape
export function main(): void {
  print("\ud83d\u{41}");
}
