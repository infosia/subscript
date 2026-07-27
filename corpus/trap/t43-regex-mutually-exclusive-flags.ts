// corpus: trap/t43-regex-mutually-exclusive-flags
// questions: Q31
// purpose: Pins regex-error for mutually exclusive dynamic flags.
// expected: regex-error at the RegExp constructor

export function main(): void {
  print("before");
  const regex: RegExp = new RegExp("a", "uv");
  print(regex.source);
}
