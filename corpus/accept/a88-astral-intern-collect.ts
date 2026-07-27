// corpus: accept/a88-astral-intern-collect
// purpose: P24 keeps Context-interned astral scalars live across collect().
// observable: astral for-of prints identically before and after collection.
// exercises: for-of-string, astral-code-point, collect, p24-context-interning

export function main(): void {
  const text: string = "😀🦀😀";
  for (const value of text) {
    print(`before:${value}`);
  }
  collect();
  for (const value of text) {
    print(`after:${value}`);
  }
}
