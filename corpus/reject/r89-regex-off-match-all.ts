// corpus: reject/r89-regex-off-match-all
// feature: regex-off
// purpose: String.matchAll is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  const matches = "x".matchAll(/x/g);
  print(`${matches}`);
}
