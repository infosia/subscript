// corpus: accept/a496-sort-keeps-the-elements-that-its-comparator-replaces
// purpose: A function-value sort keeps each element that its comparator replaces in the receiver.
// observable: The sort writes back the original elements in order, after each comparator call collected.
// exercises: function-value-intrinsic, explicit-collection, runtime-callback-rooting, sort
// questions: compiler.md §345 rules 1 and 2, §323.4, Q7, Q22
// tsc: accepts; js-comparable: no Q7: Context.collect has no JavaScript equivalent.
//
// Each comparator call replaces every element of `items` with a new Item
// of value 1000 + index, so after the first call only the sort holds the
// original elements. The comparator reads the fields before it
// collects, so its parameters are not live then. The sort reads its input before the first call and
// writes back the sorted input at the end, as JavaScript does.
// Expected: "sort 10,20,30,40,50,60,".
class Item {
  v: i32;
  constructor(v: i32) { this.v = v; }
}
const items: Item[] = [];
let junk: Item[] = [];
function byValue(a: Item, b: Item): i32 {
  const d: i32 = a.v - b.v;
  for (let j: i32 = 0; j < items.length; j += 1) { items[j] = new Item(1000 + j); }
  for (let k: i32 = 0; k < 32; k += 1) { junk = [new Item(k), new Item(k + 1)]; }
  Context.collect();
  return d;
}
export function main(): void {
  for (const v of [40, 10, 60, 30, 20, 50]) { items.push(new Item(v)); }
  const f: (a: Item, b: Item) => i32 = byValue;
  items.sort(f);
  let s: string = "";
  for (const o of items) { s += `${o.v},`; }
  print(`sort ${s}`);
}
