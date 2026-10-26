// corpus: accept/a373-undefined-result
// purpose: A script method returns undefined on a miss, and callers test it as they test Map.get.
// exercises: undefined, nullable, Map.get, Array.find, equality, nullish
// questions: compiler.md §195
// tsc: accepts; js-comparable: yes
class Box {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Store<V> {
  items: V[] = [];
  add(item: V): void {
    this.items.push(item);
  }
  get(i: i32): V | undefined {
    if (i < 0 || i >= this.items.length) {
      return undefined;
    }
    return this.items[i];
  }
}

function lookup(m: Map<string, Box>, key: string): Box | undefined {
  const hit = m.get(key);
  if (hit === undefined) {
    return undefined;
  }
  return hit;
}

export function main(): void {
  const s = new Store<Box>();
  s.add(new Box(1));
  s.add(new Box(2));
  const a = s.get(1);
  if (a !== undefined) {
    print(`hit ${a.v}`);
  }
  const miss = s.get(5);
  print(`${miss === undefined} ${miss == null} ${miss != undefined}`);
  print(`${(s.get(9) ?? new Box(-1)).v}`);
  let cur = s.get(0);
  print(`${cur === undefined}`);
  cur = s.get(7);
  print(`${cur === undefined}`);
  const fallback: Box | null = null;
  const either: Box | null = s.get(8) ?? fallback;
  print(`${either == null} ${fallback == undefined}`);
  const m = new Map<string, Box>();
  m.set("k", new Box(3));
  const found = lookup(m, "k");
  if (found !== undefined) {
    print(`found ${found.v}`);
  }
  print(`${lookup(m, "z") === undefined} ${m.get("z") === undefined}`);
  const boxes: Box[] = [new Box(4), new Box(5)];
  const f = boxes.find((b: Box): boolean => b.v === 5);
  if (f !== undefined) {
    print(`find ${f.v}`);
  }
  print(`${boxes.findLast((b: Box): boolean => b.v > 9) === undefined}`);
}
