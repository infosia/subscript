// corpus: reject/r56-json-stringify-map
// purpose: Rejects Map as JSON.stringify input instead of silently emitting {}.
// exercises: JSON.stringify, Map, rejected-input-family
// expected: S014 at stringify
// questions: Q28

export function main(): void {
  const value: Map<i32, i32> = new Map<i32, i32>();
  JSON.stringify(value);
}
