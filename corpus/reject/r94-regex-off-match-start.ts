// corpus: reject/r94-regex-off-match-start
// feature: regex-off
// purpose: RegExp.matchStart is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print(`${/(x)/.matchStart(1)}`);
}
