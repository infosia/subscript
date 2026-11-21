// corpus: accept/a495-group-by-keeps-the-element-that-its-callback-removes
// purpose: A function-value Map.groupBy keeps each element that its callback replaces in the receiver.
// observable: Each group holds the elements that the callback saw, after each callback collected.
// exercises: function-value-intrinsic, explicit-collection, runtime-callback-rooting, map-group-by
// questions: compiler.md §345 rules 1 and 2, Q7, Q24
// tsc: accepts; js-comparable: no Q7: Context.collect has no JavaScript equivalent.
//
// Each callback replaces every element of `items` with a new Item of
// value + 1, so only the operation holds the element that it passed.
// Visits: 10 (k0), 21 (k1), 32 (k0), 43 (k1).
// Expected: "k0 10,32,", "k1 21,43,", "items 14,24,34,44,".
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
function parity(x: Item): string {
  const v: i32 = x.v;
  replaceAll();
  return `k${v % 2}`;
}
export function main(): void {
  for (let i: i32 = 1; i <= 4; i += 1) { items.push(new Item(i * 10)); }
  const f: (x: Item) => string = parity;
  const groups: Map<string, Item[]> = Map.groupBy(items, f);
  for (const key of ["k0", "k1"]) {
    const g = groups.get(key);
    let s: string = "";
    if (g !== undefined) { for (const o of g) { s += `${o.v},`; } }
    print(`${key} ${s}`);
  }
  let t: string = "";
  for (const o of items) { t += `${o.v},`; }
  print(`items ${t}`);
}
