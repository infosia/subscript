// corpus: accept/a493-filter-keeps-the-element-that-its-callback-removes
// purpose: A function-value filter keeps each element that its callback replaces in the receiver.
// observable: The result holds the elements that the callback saw, after each callback collected.
// exercises: function-value-intrinsic, explicit-collection, runtime-callback-rooting, filter
// questions: compiler.md §345 rules 1 and 2, Q7, Q22
// tsc: accepts; js-comparable: no Q7: Context.collect has no JavaScript equivalent.
//
// Each callback replaces every element of `items` with a new Item of
// value + 1, so only the operation holds the element that it passed.
// Visits: 10 (items become 11 21 31 41), 21 (12 22 32 42),
// 32 (13 23 33 43), 43 (14 24 34 44). The predicate drops 21.
// Expected: "filter 10,32,43," then "items 14,24,34,44,".
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
function keep(x: Item): boolean {
  const v: i32 = x.v;
  replaceAll();
  return v % 10 !== 1;
}
export function main(): void {
  for (let i: i32 = 1; i <= 4; i += 1) { items.push(new Item(i * 10)); }
  const f: (x: Item) => boolean = keep;
  const out: Item[] = items.filter(f);
  let s: string = "";
  for (const o of out) { s += `${o.v},`; }
  print(`filter ${s}`);
  let t: string = "";
  for (const o of items) { t += `${o.v},`; }
  print(`items ${t}`);
}
