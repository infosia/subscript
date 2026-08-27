// corpus: reject/r41-map-scalar-get
// purpose: Rejects get on a scalar-valued Map with no null miss value.
// exercises: map-get-miss, scalar-value
// questions: Q24, C7
// tsc: accepts
// expected-error: use has plus getOr for a scalar-valued Map (Q24)
export function main(): void {
  const map: Map<i32, i32> = new Map<i32, i32>();
  print(`${map.get(1)}`);
}
