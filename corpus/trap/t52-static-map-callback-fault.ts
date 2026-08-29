// corpus: trap/t52-static-map-callback-fault
// purpose: A named map callback preserves its internal bounds trap.
// exercises: Array, indexing, map, static callback trap unwind
// questions: Q22
// expected-trap: index-out-of-bounds inside the named map callback

const probe: i32[] = [7];
function trapMap(value: i32, index: i32): i32 {
  return value + probe[index + 1];
}

export function main(): void {
  const values: i32[] = [1];
  values.map(trapMap);
}
