// corpus: accept/a386-transparent-type-alias/main
// purpose: A non-generic type alias is a second name of its aliased type in every type position.
// exercises: type-alias, module-import, string-literal-union
// questions: compiler.md §202
// tsc: accepts; js-comparable: yes
import { origin, Point } from "./lib";
import type { P } from "./lib";

type Early = Later;
type Later = Id;
type Id = i32;
type Ids = Id[];
type Step = (v: Id) => Id;
type MaybeItem = Item | null;
type Index = Map<string, Item>;
type Label = "ready" | "done";
type State = Label;

class Item {
  id: Id;
  label: State;
  constructor(id: Id, label: State) {
    this.id = id;
    this.label = label;
  }
}

function apply(step: Step, ids: Ids): Ids {
  const out: Ids = [];
  for (const id of ids) {
    out.push(step(id));
  }
  return out;
}

function find(index: Index, key: string): MaybeItem {
  const item = index.get(key);
  return item === undefined ? null : item;
}

function describe(s: State): string {
  switch (s) {
    case "ready":
      return "r";
    case "done":
      return "d";
  }
}

export function main(): void {
  const e: Early = 7;
  const doubled = apply((v: Id): Id => v * 2, [1, 2, 3]);
  print(`${e} ${doubled.join(",")}`);

  const index: Index = new Map<string, Item>();
  index.set("a", new Item(1, "ready"));
  const hit: MaybeItem = find(index, "a");
  const miss: MaybeItem = find(index, "b");
  print(`${hit !== null ? hit.id : -1} ${miss === null} ${hit !== null ? describe(hit.label) : "-"}`);

  const p: P = origin();
  const q: Point = p;
  q.x = 2;
  print(`${p.x} ${p.y}`);
}
