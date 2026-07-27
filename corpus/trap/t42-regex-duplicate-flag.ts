// corpus: trap/t42-regex-duplicate-flag
// questions: Q31
// purpose: Pins regex-error for a duplicated dynamic flag.
// expected: regex-error at the RegExp constructor

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("a", "gg");
  print(regex.source);
}
