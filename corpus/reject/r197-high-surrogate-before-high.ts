// corpus: reject/r197-high-surrogate-before-high
// purpose: Rejects a high surrogate before another high surrogate.
// exercises: string-escape, lone-surrogate
// questions: Q5, §96
// tsc: accepts
// expected-error: S100 at the first surrogate escape
export function main(): void {
  print("\ud83d\ud83d");
}
