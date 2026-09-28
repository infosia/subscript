// corpus: reject/r251-map-boundary-struct-get
// purpose: Rejects Map.get when a boundary value requires a nullable box.
// exercises: boundary-struct-value, map-get-miss
// questions: compiler.md §123
// tsc: accepts
// expected-error: S014 at line 8
function read(map: Map<i32, SubByValueI64Pair>): void {
  map.get(1);
}
