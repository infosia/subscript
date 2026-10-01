// corpus: accept/a308-reference-search-miss
// purpose: Tests reference-element search misses with loose null equality.
// exercises: Map.get, Array.find, nullable, equality
// questions: compiler.md §144, C22
// tsc: accepts; js-comparable: yes
class Box { value: i32 = 7; }
export function main(): void {
  const box = new Box();
  const values = new Map<string, Box>();
  values.set("hit", box);
  const mapMiss = values.get("miss");
  print(`${mapMiss == null}`);
  const boxes: Box[] = [box];
  const findMiss = boxes.find((value: Box): boolean => value.value == 0);
  print(`${findMiss == null}`);
  const mapHit = values.get("hit");
  print(`${mapHit == null}`);
  if (mapHit != null) { print(`${mapHit.value}`); }
  const findHit = boxes.find((value: Box): boolean => value.value == 7);
  print(`${findHit == null}`);
  if (findHit != null) { print(`${findHit.value}`); }
}
