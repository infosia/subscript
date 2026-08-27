// corpus: reject/r82-regex-last-index
// questions: Q31
// tsc: accepts
// purpose: Rejects lastIndex because global matching cannot expose mutable
//          state that drives the unrepresentable exec result.
// exercises: RegExp.lastIndex, mutable-global-match-state
// expected-error: S014 naming mutable global-match state
export function main(): void {
  const index: i32 = /x/g.lastIndex;
  print(`${index}`);
}
