// corpus: reject/r27-string-match
// purpose: Rejects `match`: feature-off builds lack P23; feature-on
//          builds cannot represent its strict-TS optional index.
// exercises: rejected-string-subset, string-methods
// questions: Q21
// expected-error: match requires RegExp; out of subset

export function main(): void {
  const match = "hello".match(/l/);
  if (match !== null) {
    const index: i32 = match.index;
    print(`${index}`);
  }
}
