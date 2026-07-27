// corpus: accept/a85-for-of-repeated-astral
// purpose: P24 repeatedly returns one Context-interned astral scalar.
// observable: every repeated astral code point prints unchanged.
// exercises: for-of-string, astral-code-point, p24-context-interning

export function main(): void {
  const text: string = "😀😀😀😀";
  for (const value of text) {
    print(`repeated:${value}`);
  }
}
