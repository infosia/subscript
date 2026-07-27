// corpus: reject/r91-regex-off-replace
// feature: regex-off
// purpose: Regex-backed String.replace is unavailable without P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print("x".replace(/x/, "y"));
}
