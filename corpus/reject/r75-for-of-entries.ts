// corpus: reject/r75-for-of-entries
// purpose: Rejects entries() even in direct for-of subject position.
// exercises: missing-tuple-type, for-of-subject
// questions: Q30
// tsc: accepts
// expected-error: entries() yields a pair and the language has no tuple type
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  for (const entry of map.entries()) {
    print(`${entry}`);
  }
}
