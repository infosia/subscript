// corpus: reject/r57-json-stringify-set
// purpose: Rejects Set as JSON.stringify input instead of silently emitting {}.
// exercises: JSON.stringify, Set, rejected-input-family
// expected: S014 at stringify
// questions: Q28

export function main(): void {
  const value: Set<i32> = new Set<i32>();
  JSON.stringify(value);
}
