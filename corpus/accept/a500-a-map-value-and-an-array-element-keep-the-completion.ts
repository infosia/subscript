// corpus: accept/a500-a-map-value-and-an-array-element-keep-the-completion
// purpose: Completed tasks whose handles are Map values and array elements of reachable containers keep their completions.
// observable: Each await reads the fields of the completion value, after allocations, polls, and collections.
// exercises: async-handle, completed-task, counted-map, counted-array, explicit-collection, automatic-collection
// questions: compiler.md §348 rules 1 to 3, §342, §70, Q7, C8
// tsc: accepts; js-comparable: no C8 Q7: The Context coroutine and memory API has no JavaScript shim.
//
// The Map, the local array, and the array field of `bag` are the only
// holders of the handles. Map order is insertion order: 11, then 12.
// Box(n) gives "n boxn": items[2] is 3n and items[1] is 2n.
class Box {
  a: i32;
  b: string;
  items: i32[];
  constructor(a: i32) { this.a = a; this.b = `box${a}`; this.items = [a, a * 2, a * 3]; }
}
async function makeLater(n: i32): Promise<Box> { await Context.suspend(); return new Box(n); }
function churn(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const t: i32[] = [i, i + 1]; s += t[1]; }
  return s;
}
class Bag {
  tasks: Promise<Box>[];
  constructor() { this.tasks = []; }
}
export async function main(): Promise<void> {
  const m: Map<i32, Promise<Box>> = new Map<i32, Promise<Box>>();
  m.set(1, makeLater(11));
  m.set(2, makeLater(12));
  const arr: Promise<Box>[] = [];
  arr.push(makeLater(13));
  const bag: Bag = new Bag();
  bag.tasks.push(makeLater(14));
  for (let k: i32 = 0; k < 3; k += 1) { await Context.suspend(); churn(20); }
  Context.collect();
  for (const t of m.values()) {
    const b: Box = await t;
    churn(20);
    print(`map ${b.a} ${b.b} ${b.items[2]}`);
  }
  Context.collect();
  const c: Box = await arr[0];
  print(`array ${c.a} ${c.b} ${c.items[1]}`);
  Context.collect();
  const d: Box = await bag.tasks[0];
  print(`bag ${d.a} ${d.b} ${d.items[1]}`);
  print("done");
}
