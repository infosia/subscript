// corpus: trap/t45-regex-sticky-last-index
// questions: Q31
// purpose: Pins the missing RegExp.lastIndex language gap for a dynamic sticky flag.
// expected: regex-error at the RegExp constructor naming RegExp.lastIndex

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("a", "y");
  print(regex.source);
}
