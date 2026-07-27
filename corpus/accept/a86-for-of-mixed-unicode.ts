// corpus: accept/a86-for-of-mixed-unicode
// purpose: P24 switches between tagged BMP and allocated astral handles.
// observable: mixed code points print in source order without representation leaks.
// exercises: for-of-string, bmp-code-points, astral-code-points, p24-mixed-handles

export function main(): void {
  const text: string = "A😀é🦀Z";
  for (const value of text) {
    print(`mixed:${value}`);
  }
}
