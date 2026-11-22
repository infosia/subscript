// corpus: accept/a499-a-task-group-and-promise-all-keep-their-completions
// purpose: Completed tasks in a TaskGroup and in a Promise.all keep their completions across collections.
// observable: The join and the await read fields that the completed tasks wrote or returned, after allocations, polls, and collections.
// exercises: task-group, promise-all, async-handle, completed-task, explicit-collection, automatic-collection
// questions: compiler.md §348 rules 1 to 3 and 5, §342, §170, §70, Q7, C8
// tsc: accepts; js-comparable: no C8 Q7: The Context coroutine and memory API has no JavaScript shim.
//
// TaskGroup.add takes Promise<void>, so the group tasks write into their
// receiver. The receiver of the tasks that addGrowth adds is held only
// by their frames. `kept` gets grow(1) then grow(2): items 1 2 3 1 2,
// b "box1+1+2". Promise.all returns Box(3) and Box(4).
class Box {
  a: i32;
  b: string;
  items: i32[];
  constructor(a: i32) { this.a = a; this.b = `box${a}`; this.items = [a, a * 2, a * 3]; }
  async grow(step: i32): Promise<void> {
    await Context.suspend();
    this.items.push(step);
    this.b = `${this.b}+${step}`;
  }
}
async function makeLater(n: i32): Promise<Box> { await Context.suspend(); return new Box(n); }
function churn(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const t: i32[] = [i, i + 1]; s += t[1]; }
  return s;
}
function addGrowth(group: TaskGroup, n: i32): void {
  const box: Box = new Box(n);
  group.add(box.grow(5));
  group.add(box.grow(6));
}
export async function main(): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  const kept: Box = new Box(1);
  group.add(kept.grow(1));
  group.add(kept.grow(2));
  addGrowth(group, 2);
  const all: Promise<Box[]> = Promise.all([makeLater(3), makeLater(4)]);
  for (let k: i32 = 0; k < 3; k += 1) { await Context.suspend(); churn(20); }
  Context.collect();
  await group.join();
  churn(20);
  Context.collect();
  print(`group ${kept.a} ${kept.b} ${kept.items.length} ${kept.items[4]}`);
  const boxes: Box[] = await all;
  churn(20);
  Context.collect();
  print(`all ${boxes[0].a} ${boxes[0].b} ${boxes[1].b} ${boxes[1].items[2]}`);
  print("done");
}
