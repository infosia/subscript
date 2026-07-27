// corpus: reject/r86-regex-literal-replace-all-without-global
// questions: Q31
// purpose: Rejects replaceAll with a statically visible non-global literal.
// expected-error: S100 requiring the g flag

export function main(): void {
  print("aaa".replaceAll(/a/, "Z"));
}
