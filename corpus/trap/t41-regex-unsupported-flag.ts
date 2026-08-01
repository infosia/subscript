// corpus: trap/t41-regex-unsupported-flag
// questions: Q31
// purpose: Pins regex-error for an unsupported dynamic flag.
// exercises: RegExp-constructor, unsupported-flag, regex-error

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("a", "q");
  print(regex.source);
}
