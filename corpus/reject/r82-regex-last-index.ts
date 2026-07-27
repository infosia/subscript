// corpus: reject/r82-regex-last-index
// feature: regex
// purpose: Rejects lastIndex because global matching cannot expose mutable
//          state that drives the unrepresentable exec result.
// expected-error: S014 naming mutable global-match state

export function main(): void {
  const index: i32 = /x/g.lastIndex;
  print(`${index}`);
}
