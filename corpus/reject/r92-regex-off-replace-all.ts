// corpus: reject/r92-regex-off-replace-all
// feature: regex-off
// purpose: Regex-backed String.replaceAll is unavailable without P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print("x".replaceAll(/x/g, "y"));
}
