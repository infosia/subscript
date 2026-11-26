// corpus: reject/r565-free-string
// purpose: Context.free of a string is rejected; a string is a value with no identity.
// exercises: Context.free, string
// questions: Q6, compiler.md §358 rule 8
// tsc: rejects TS2345
// expected-error: S100, the argument expects `object`, got `string`

export function main(): void {
  const text: string = "abc";
  Context.free(text);
}
