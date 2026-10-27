// corpus: accept/a377-generic-construction-inference
// purpose: A generic constructor and a generic method infer their type arguments from arguments and from context.
// exercises: generics, type-inference, constructor, Map, Set
// questions: compiler.md §197
// tsc: accepts; js-comparable: yes
class Box<T> {
  v: T;
  constructor(v: T) {
    this.v = v;
  }
}

class Pair<A, B> {
  a: A;
  b: B;
  constructor(a: A, b: B) {
    this.a = a;
    this.b = b;
  }
  swap(): Pair<B, A> {
    return new Pair(this.b, this.a);
  }
}

class Item {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

class Registry {
  byName: Map<string, Item> = new Map();
  tags: Set<string> = new Set();
  wrap<T>(value: T): Box<T> {
    return new Box(value);
  }
}

function total(boxes: Box<i32>[]): i32 {
  let sum: i32 = 0;
  for (const b of boxes) {
    sum += b.v;
  }
  return sum;
}

export function main(): void {
  const n: i64 = 5;
  const a = new Box(n);
  const b: Box<i64> = new Box(7);
  const c = new Box("s");
  const explicit = new Box<string>("s");
  print(`${a.v} ${b.v} ${c.v === explicit.v}`);
  const boxes: Box<i32>[] = [new Box(1), new Box(2)];
  print(`${total(boxes)} ${total([new Box(3)])}`);
  const p = new Pair(1, "x").swap();
  print(`${p.a} ${p.b}`);
  const r = new Registry();
  r.byName.set("k", new Item(3));
  r.tags.add("t");
  const local: Map<string, Item> = new Map();
  local.set("j", new Item(4));
  const hit = r.byName.get("k");
  print(`${hit !== undefined ? hit.id : -1} ${r.tags.size} ${local.size}`);
  const w = r.wrap(new Item(8));
  const ww = r.wrap(w);
  print(`${w.v.id} ${ww.v.v.id}`);
}
