// corpus: trap/t53-value-map-callback-fault
// purpose: A function-value map callback preserves its internal bounds trap.
// exercises: Array, indexing, map, function-value callback trap unwind
// questions: Q22
// expected-trap: index-out-of-bounds inside the function-value map callback

const probe: i32[] = [7];
function trapMap(value: i32, index: i32): i32 {
  return value + probe[index + 1];
}

export function main(): void {
  const values: i32[] = [1];
  const callback: (value: i32, index: i32) => i32 = trapMap;
  values.map(callback);
}
