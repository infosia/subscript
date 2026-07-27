// corpus: reject/r87-regex-off-source
// feature: regex-off
// purpose: RegExp.source is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print(/x/.source);
}
