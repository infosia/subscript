// corpus: reject/r205-array-spread-bare-map
// purpose: Rejects a bare Map as an array-literal spread operand.
// exercises: array-spread, map-iteration, invariant-5
// questions: Q30, compiler section 104
// tsc: accepts
// expected-error: S014 naming the pair TypeScript binds
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  const keys = [...map];
  print(`${keys.length}`);
}
