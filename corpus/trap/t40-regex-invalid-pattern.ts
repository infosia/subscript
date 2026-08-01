// corpus: trap/t40-regex-invalid-pattern
// questions: Q31
// purpose: Pins regex-error for a dynamic pattern that does not compile.
// exercises: RegExp-constructor, dynamic-pattern, regex-error

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("(", "");
  print(regex.source);
}
