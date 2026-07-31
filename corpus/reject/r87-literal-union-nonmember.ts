// corpus: reject/r87-literal-union-nonmember
// purpose: Rejects a non-member literal in a Q32 alias context.
// exercises: string-literal-union, closed-member-set
// questions: Q32
// expected-error: S100 type mismatch at the non-member literal

type IndexFormat = "uint16" | "uint32";
export function main(): void {
  const format: IndexFormat = "float32";
  print(`${format}`);
}
