// corpus: accept/a87-for-of-distinct-astral
// purpose: P24 interns one ordinary string handle per distinct astral scalar.
// observable: several distinct astral code points print in source order.
// exercises: for-of-string, distinct-astral-code-points, p24-context-interning
// questions: Q30

export function main(): void {
  const text: string = "😀🦀𐍈🚀";
  for (const value of text) {
    print(`distinct:${value}`);
  }
}
