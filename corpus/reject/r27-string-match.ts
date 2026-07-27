// corpus: reject/r27-string-match
// purpose: Rejects `match`: strict TS makes its index optional, which
//          the language cannot represent as its required i32 result.
// exercises: rejected-string-subset, string-methods
// questions: Q31
// expected-error: S014 naming the optional-index type gap

export function main(): void {
  const match = "hello".match(/l/);
  if (match !== null) {
    const index: i32 = match.index;
    print(`${index}`);
  }
}
