// corpus: reject/r88-literal-union-inline
// purpose: Keeps inline string-literal unions in the rejected general-union space.
// exercises: string-literal-union, alias-only
// questions: Q32, C7
// expected-error: S011 at the inline parameter annotation

function useFormat(format: "uint16" | "uint32"): void {
  print(format);
}

export function main(): void {
  useFormat("uint16");
}
