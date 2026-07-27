// corpus: reject/r93-regex-off-split
// feature: regex-off
// purpose: Regex-backed String.split is unavailable without P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  const pieces: string[] = "x".split(/x/);
  print(`${pieces.length}`);
}
