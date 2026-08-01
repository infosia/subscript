// corpus: reject/r84-regex-sticky-last-index
// questions: Q31
// purpose: Rejects sticky matching because it requires mutable lastIndex.
// exercises: sticky-RegExp, RegExp.lastIndex
// expected-error: S014 naming the lastIndex language gap

export function main(): void {
  const regex: RegExp = /a/y;
  print(regex.source);
}
