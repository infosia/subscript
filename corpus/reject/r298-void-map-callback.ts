// corpus: reject/r298-void-map-callback
// purpose: Rejects a map callback with no return value.
// exercises: void, array-map
// questions: collisions.md C21
// tsc: accepts
// expected-error: S100 at line 10, Rejects a map callback with no return value.

export function main(): void {
  const xs: FixedArray<i32, 1> = [1];
  xs.map((v: i32): void => {});
}
