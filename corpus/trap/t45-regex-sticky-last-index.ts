// corpus: trap/t45-regex-sticky-last-index
// questions: Q31
// purpose: Pins the missing RegExp.lastIndex language gap for a dynamic sticky flag.
// exercises: RegExp-constructor, sticky-flag, RegExp.lastIndex, regex-error

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("a", "y");
  print(regex.source);
}
