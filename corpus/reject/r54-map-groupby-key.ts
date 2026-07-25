// corpus: reject/r54-map-groupby-key
// purpose: Applies the Q24 key whitelist to the key inferred from a
//          Map.groupBy callback.
// expected: S014 at the callback
// questions: Q27, Q24

export function main(): void {
  Map.groupBy([1, 2], (value: i32): i32[] => [value]);
}
