// corpus: reject/r214-pattern-over-entries
// purpose: Rejects a pair pattern over the entries view.
// exercises: binding-pattern, missing-tuple-type, for-of
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S014 naming the missing tuple type
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  for (const [key, value] of map.entries()) {
    print(`${key} ${value}`);
  }
}
