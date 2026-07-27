// corpus: reject/r79-assign-entries
// purpose: Rejects entries() as an ordinary value expression.
// exercises: missing-tuple-type, entries-rejected-everywhere
// questions: Q30
// expected-error: entries() yields a pair and the language has no tuple type

export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  const entries = map.entries();
  print(`${entries}`);
}
