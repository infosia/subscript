// corpus: accept/a404-string-undefined-result
// purpose: `get`, `find`, `findLast`, `pop`, and `shift` give `string | undefined` on a string element, and equality with a non-null operand narrows a nullable string and a nullable class in each operand order.
// exercises: nullable-string, undefined, Map.get, Array.find, Array.findLast, Array.pop, Array.shift, nullish-coalescing, null-narrowing, loose-equality
// questions: compiler.md §219, C7
// tsc: accepts; js-comparable: yes
class Node2 {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

class Holder {
  name: string | null = "held";
  owner: Node2 | null = null;

  matches(text: string): i32 {
    if (this.name === text) {
      return this.name.length;
    }
    return -1;
  }
}

let shared: string | null = "shared";

function mk(text: string): string {
  return text + "!";
}

function maybe(text: string, present: boolean): string | null {
  return present ? text : null;
}

function mapGet(): void {
  const m = new Map<string, string>();
  m.set("a", "apple");
  m.set("b", mk(""));
  const hit = m.get("a");
  print(`${hit === undefined}`);
  if (hit !== undefined) {
    print(hit);
  }
  print(`${m.get("z") === undefined}`);
  print(m.get("a") ?? "none");
  print(m.get("z") ?? "none");
}

function search(): void {
  const xs: string[] = ["ant", "bee", "bat"];
  const first = xs.find((x: string): boolean => x.startsWith("b"));
  const last = xs.findLast((x: string): boolean => x.startsWith("b"));
  print(first ?? "none");
  print(last ?? "none");
  print(xs.find((x: string): boolean => x === "cow") ?? "none");
  print(xs.findLast((x: string): boolean => x === "cow") ?? "none");
  print(`${xs.find((x: string): boolean => x === "cow") === undefined}`);
}

function removal(): void {
  const xs: string[] = ["one", "two", "three"];
  print(xs.pop() ?? "none");
  print(xs.shift() ?? "none");
  const rest = xs.pop();
  if (rest !== undefined) {
    print(`${rest.length}`);
  }
  print(xs.pop() ?? "empty pop");
  print(xs.shift() ?? "empty shift");
  print(`${xs.pop() === undefined} ${xs.shift() === undefined}`);
  print(`${xs.pop()?.length ?? -1}`);
  xs.pop();
  xs.shift();
  print(`${xs.length}`);
}

function stringEquality(q: string | null, text: string): void {
  if (q === "lit") {
    print(`left ${q.length}`);
  }
  if ("lit" === q) {
    print(`right ${q.length}`);
  }
  if (q !== text) {
    print("differs");
  } else {
    print(`same ${q.length}`);
  }
  if (text != q) {
    print("differs loose");
  } else {
    print(`same loose ${q.length}`);
  }
  if (q == text) {
    print(`equal loose ${q.length}`);
  }
}

function earlyReturn(q: string | null): i32 {
  if (q !== mk("x")) {
    return -1;
  }
  return q.length;
}

function undefinedResult(xs: string[]): i32 {
  const r = xs.pop();
  if (r === "x") {
    return r.length;
  }
  return 0;
}

function classEquality(a: Node2 | null, b: Node2): void {
  if (a === b) {
    print(`left ${a.id}`);
  }
  if (b === a) {
    print(`right ${a.id}`);
  }
  if (a !== b) {
    print("other node");
  } else {
    print(`same ${a.id}`);
  }
  if (b == a) {
    print(`loose ${a.id}`);
  }
}

function sharedLocations(h: Holder, other: Node2): void {
  print(`${h.matches("held")} ${h.matches("x")}`);
  h.owner = other;
  h.owner = maybe("a", true) === "a" ? other : null;
  if (h.owner === other) {
    print(`field ${h.owner.id}`);
  }
  if (shared === "shared") {
    print(`global ${shared.length}`);
  }
}

function loopAndValue(words: string[]): void {
  let total = 0;
  let found: string | null = null;
  for (const word of words) {
    found = maybe(word, word !== "skip");
    if (found === word) {
      total += found.length;
    }
  }
  print(`${total}`);
  let filled: string | null = null;
  filled ??= "set";
  print(`${filled.length}`);
  const q = maybe("value", true);
  const size = q === "value" ? q.length : 0;
  const both = q === "value" && q.length > 2;
  print(`${size} ${both}`);
}

export function main(): void {
  mapGet();
  search();
  removal();
  stringEquality("lit", "lit");
  stringEquality(mk("lit"), "lit");
  stringEquality(null, "lit");
  print(`${earlyReturn("x!")} ${earlyReturn("y")} ${earlyReturn(null)}`);
  print(`${undefinedResult(["x"])} ${undefinedResult(["y"])} ${undefinedResult([])}`);
  const node = new Node2(7);
  classEquality(node, node);
  classEquality(new Node2(7), node);
  classEquality(null, node);
  sharedLocations(new Holder(), node);
  loopAndValue(["a", "skip", "ccc"]);
}
