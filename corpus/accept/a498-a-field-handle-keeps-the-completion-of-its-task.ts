// corpus: accept/a498-a-field-handle-keeps-the-completion-of-its-task
// purpose: A completed task whose handle is in a field of a reachable object keeps its completion across collections.
// observable: Each await reads the fields of the completion value, or catches the completion Error, after allocations, polls, and collections.
// exercises: async-handle, completed-task, counted-field, async-instance-method, explicit-collection, automatic-collection
// questions: compiler.md §348 rules 1 to 3, §342, §70, Q7, C8
// tsc: accepts; js-comparable: no C8 Q7: The Context coroutine and memory API has no JavaScript shim.
//
// `kept` is a global, `local` and `failing` are locals of main, and
// `w.task` holds a task that returns `w` itself.
// global: Box(7) gives "7 box7 21". local: Box(8) gives "8 box8 16".
// worker: values 5 6 7 8, and load adds 10 to values[0]: "15 8".
class Box {
  a: i32;
  b: string;
  items: i32[];
  constructor(a: i32) { this.a = a; this.b = `box${a}`; this.items = [a, a * 2, a * 3]; }
}
async function makeLater(n: i32): Promise<Box> { await Context.suspend(); return new Box(n); }
async function failLater(tag: string): Promise<Box> { await Context.suspend(); throw new Error(tag); }
function churn(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const t: i32[] = [i, i + 1]; s += t[1]; }
  return s;
}
class Holder {
  task: Promise<Box>;
  constructor(task: Promise<Box>) { this.task = task; }
}
async function none(): Promise<Worker | null> { return null; }
class Worker {
  values: i32[];
  task: Promise<Worker | null>;
  constructor(n: i32) { this.values = [n, n + 1, n + 2, n + 3]; this.task = none(); }
  start(): void { this.task = this.load(); }
  async load(): Promise<Worker | null> { await Context.suspend(); this.values[0] += 10; return this; }
}
let kept: Holder | null = null;
export async function main(): Promise<void> {
  kept = new Holder(makeLater(7));
  const local: Holder = new Holder(makeLater(8));
  const failing: Holder = new Holder(failLater("kept error"));
  const w: Worker = new Worker(5);
  w.start();
  for (let k: i32 = 0; k < 3; k += 1) { await Context.suspend(); churn(20); }
  Context.collect();
  const k0: Holder | null = kept;
  if (k0 !== null) {
    const b: Box = await k0.task;
    print(`global ${b.a} ${b.b} ${b.items[2]}`);
  }
  churn(20);
  const c: Box = await local.task;
  print(`local ${c.a} ${c.b} ${c.items[1]}`);
  try { await failing.task; } catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  churn(20);
  Context.collect();
  const loaded: Worker | null = await w.task;
  if (loaded !== null) { print(`worker ${loaded.values[0]} ${loaded.values[3]}`); }
  print("done");
}
