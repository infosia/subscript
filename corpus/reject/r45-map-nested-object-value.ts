// corpus: reject/r45-map-nested-object-value
// purpose: Keeps boundary-only object from leaking through a nested key type.
// exercises: map-key-resolution, boundary-only-object
// questions: Q24, C7
// tsc: accepts
// expected-error: nested Map values are general declarations, not key positions
export function main(): void {
  const map: Map<Map<i32, object>, i32> = new Map<Map<i32, object>, i32>();
  print(`${map.size}`);
}
