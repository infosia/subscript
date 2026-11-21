// corpus: accept/a494-find-keeps-the-element-that-its-callback-removes
// purpose: Function-value find and findLast return the element that their callback replaced in the receiver.
// observable: Each result is the element that the callback saw, after each callback collected.
// exercises: function-value-intrinsic, explicit-collection, runtime-callback-rooting, find, findLast
// questions: compiler.md §345 rules 1 and 2, Q7, Q22
// tsc: accepts; js-comparable: no Q7: Context.collect has no JavaScript equivalent.
//
// Each callback replaces every element of `items` with a new Item of
// value + 1, so only the operation holds the element that it passed.
// find over 10 20 30 40 with v >= 30: visits 10, 21, 32; returns 32.
// findLast over 5 15 25 35 with v < 20: visits 35, 26, 17; returns 17.
// Expected: "find 32", "findLast 17", "items 8,18,28,38,".
class Item {
  v: i32;
  constructor(v: i32) { this.v = v; }
}
const items: Item[] = [];
let junk: Item[] = [];
function replaceAll(): void {
  for (let j: i32 = 0; j < items.length; j += 1) { items[j] = new Item(items[j].v + 1); }
  for (let k: i32 = 0; k < 32; k += 1) { junk = [new Item(k), new Item(k + 1)]; }
  Context.collect();
}
function high(x: Item): boolean {
  const v: i32 = x.v;
  replaceAll();
  return v >= 30;
}
function low(x: Item): boolean {
  const v: i32 = x.v;
  replaceAll();
  return v < 20;
}
export function main(): void {
  for (let i: i32 = 1; i <= 4; i += 1) { items.push(new Item(i * 10)); }
  const f: (x: Item) => boolean = high;
  const a = items.find(f);
  if (a !== undefined) { print(`find ${a.v}`); } else { print("find none"); }
  for (let i: i32 = 0; i < 4; i += 1) { items[i] = new Item(i * 10 + 5); }
  const g: (x: Item) => boolean = low;
  const b = items.findLast(g);
  if (b !== undefined) { print(`findLast ${b.v}`); } else { print("findLast none"); }
  let t: string = "";
  for (const o of items) { t += `${o.v},`; }
  print(`items ${t}`);
}
