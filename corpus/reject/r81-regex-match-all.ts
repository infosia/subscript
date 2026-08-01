// corpus: reject/r81-regex-match-all
// questions: Q31
// purpose: Rejects matchAll because it needs a Q30 fusion decision and
//          still yields a result object at every step.
// exercises: String.matchAll, iterator-fusion, match-result-object
// expected-error: S014 naming fusion and the yielded object

export function main(): void {
  const matches = "x".matchAll(/x/g);
  print(`${matches}`);
}
