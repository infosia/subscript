// corpus: trap/t44-regex-replace-all-without-global
// questions: Q31
// purpose: Pins regex-error when an opaque RegExp value lacks the g flag.
// exercises: String.replaceAll, opaque-RegExp, global-flag, regex-error

function replaceWith(regex: RegExp): void {
  print("before");
  print("aaa".replaceAll(regex, "Z"));
  print("after");
}

export function main(): void {
  replaceWith(/a/);
}
