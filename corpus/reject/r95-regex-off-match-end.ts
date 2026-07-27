// corpus: reject/r95-regex-off-match-end
// feature: regex-off
// purpose: RegExp.matchEnd is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print(`${/(x)/.matchEnd(1)}`);
}
