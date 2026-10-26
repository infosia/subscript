// corpus: reject/r435-map-get-nullable-value
// purpose: Map.get on a nullable value type is rejected; a miss and a stored null have one representation.
// exercises: undefined, Map.get, nullable
// questions: compiler.md §195
// tsc: accepts
// expected-error: S014, the result needs both `null` and `undefined`
class Box {
  v: i32 = 1;
}
export function main(): void {
  const m = new Map<string, Box | null>();
  m.set("k", null);
  print(`${m.get("k") === undefined} ${m.get("z") === undefined}`);
}
