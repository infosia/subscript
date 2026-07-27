// corpus: reject/r88-regex-off-flags
// feature: regex-off
// purpose: RegExp.flags is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print(/x/gi.flags);
}
