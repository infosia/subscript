// corpus: accept/a269-array-of-and-map-copy
// purpose: Fixed-arity Array.of and independent shallow Map copies in insertion order.
// exercises: array-of, map-copy, reference-identity
// questions: Q22, Q24
// tsc: accepts; js-comparable: yes
class Box {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function firstValue(map: Map<string, i32>): i32 {
  for (const value of map.values()) { return value; }
  return 0;
}
export function main(): void {
  const empty = Array.of<i32>();
  const typed = Array.of<i32>(7);
  const inferred = Array.of(7);
  print(`${empty.length} ${typed[0]} ${inferred[0]}`);
  const source: Map<string, i32> = new Map<string, i32>();
  source.set("a", 1);
  source.set("b", 2);
  const copy = new Map(source);
  const explicit = new Map<string, i32>(source);
  copy.set("c", 3);
  source.delete("a");
  let copyKeys: string = "";
  for (const key of copy.keys()) { copyKeys += key; }
  let sourceKeys: string = "";
  for (const key of source.keys()) { sourceKeys += key; }
  print(`${copyKeys} ${sourceKeys} ${copy.size} ${copy === source} ${firstValue(copy)}`);
  source.set("b", 20);
  for (const value of copy.values()) { print(`${value}`); }
  copy.set("a", 10);
  print(`${firstValue(explicit)} ${source.has("c")} ${source.has("a")} ${firstValue(explicit)}`);
  const value = new Box(4);
  const refs: Map<string, Box> = new Map<string, Box>();
  refs.set("shared", value);
  const refsCopy = new Map(refs);
  const original = refs.get("shared") ?? new Box(0);
  const copied = refsCopy.get("shared") ?? new Box(0);
  print(`${original === copied}`);
  value.value = 9;
  const shared = refsCopy.get("shared") ?? new Box(0);
  print(`${shared.value}`);
  const blank = new Map<string, i32>();
  print(`${new Map(blank).size}`);
}
