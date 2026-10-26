// corpus: accept/a374-undefined-nullish-null
// purpose: A nullish fallback to the null literal turns an undefined-spelled result into a null-spelled value.
// exercises: undefined, nullable, nullish, Map.get
// questions: compiler.md §195.4 rule 1
// tsc: accepts; js-comparable: yes
class Box {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Store {
  items: Box[] = [];
  get(i: i32): Box | undefined {
    if (i < 0 || i >= this.items.length) {
      return undefined;
    }
    return this.items[i];
  }
}

function slot(s: Store, i: i32): Box | null {
  return s.get(i) ?? null;
}

export function main(): void {
  const s = new Store();
  s.items.push(new Box(7));
  const hit = slot(s, 0);
  const miss = slot(s, 3);
  print(`${hit === null} ${miss === null}`);
  if (hit !== null) {
    print(`hit ${hit.v}`);
  }
  const m = new Map<string, Box>();
  m.set("a", new Box(2));
  const n: Box | null = m.get("b") ?? null;
  print(`${n === null}`);
  const k = m.get("a") ?? null;
  print(`${k !== null}`);
  const again: Box | null = n ?? null;
  print(`${again === null}`);
}
