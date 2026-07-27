// corpus: reject/r81-regex-match-all
// feature: regex
// purpose: Rejects matchAll because it needs a Q30 fusion decision and
//          still yields a result object at every step.
// expected-error: S014 naming fusion and the yielded object

export function main(): void {
  const matches = "x".matchAll(/x/g);
  print(`${matches}`);
}
