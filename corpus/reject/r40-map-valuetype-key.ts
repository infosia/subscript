// corpus: reject/r40-map-valuetype-key
// purpose: Rejects a by-value @ValueType key with no identity hash.
// exercises: map-key-whitelist, value-class
// questions: Q24, Q22, C2
// tsc: accepts
// expected-error: `Point` is not a permitted Map/Set key kind (Q24)
@ValueType
class Point {
  x: i32;
  constructor(x: i32) {
    this.x = x;
  }
}

export function main(): void {
  const map: Map<Point, i32> = new Map<Point, i32>();
  print(`${map.size}`);
}
