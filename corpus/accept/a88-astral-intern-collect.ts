// corpus: accept/a88-astral-intern-collect
// purpose: P24 keeps Context-interned astral scalars live across Context.collect().
// observable: astral for-of prints identically before and after collection.
// exercises: for-of-string, astral-code-point, collect, p24-context-interning
// questions: Q30, Q7
// tsc: accepts; js-comparable: no Q7: The Context memory API has no JavaScript shim.
export function main(): void {
  const text: string = "😀🦀😀";
  for (const value of text) {
    print(`before:${value}`);
  }
  Context.collect();
  for (const value of text) {
    print(`after:${value}`);
  }
}
