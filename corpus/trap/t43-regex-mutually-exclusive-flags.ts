// corpus: trap/t43-regex-mutually-exclusive-flags
// questions: Q31
// purpose: Pins regex-error for mutually exclusive dynamic flags.
// exercises: RegExp-constructor, mutually-exclusive-flags, regex-error

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("a", "uv");
  print(regex.source);
}
