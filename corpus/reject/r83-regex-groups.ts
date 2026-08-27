// corpus: reject/r83-regex-groups
// questions: Q31
// tsc: accepts
// purpose: Rejects RegExpMatchArray.groups because the language has no
//          object with dynamic string keys.
// exercises: RegExpMatchArray.groups, dynamic-string-keys
// expected-error: S014 naming the dynamic-key object gap
function named(match: RegExpMatchArray): string {
  return match.groups!["word"];
}

export function main(): void {
  print(named(/x/.exec("x")!));
}
