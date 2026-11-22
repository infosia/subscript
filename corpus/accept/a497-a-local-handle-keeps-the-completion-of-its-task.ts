// corpus: accept/a497-a-local-handle-keeps-the-completion-of-its-task
// purpose: A completed task whose handle is only in a local keeps its completion across collections.
// observable: Each await reads the fields of the object that the task returned, after allocations, polls, and collections.
// exercises: async-handle, completed-task, explicit-collection, automatic-collection
// questions: compiler.md §348 rules 1 to 3, §342, §70, Q7, C8
// tsc: accepts; js-comparable: no C8 Q7: The Context coroutine and memory API has no JavaScript shim.
//
// The object that each task returns has no holder other than the
// completion. `now` completes at its call, `later` after one
// suspension, and `viaSync` completes in a synchronous caller that
// collects while it holds the handle.
// For n: "n boxn 3n (n+100) box(n+100) 2(n+100) box(n+200) 3(n+200)".
// Sum of x.a + y.a + z.a for n = 0..3: 3 * 6 + 4 * 300 = 1218.
class Box {
  a: i32;
  b: string;
  items: i32[];
  constructor(a: i32) { this.a = a; this.b = `box${a}`; this.items = [a, a * 2, a * 3]; }
}
async function make(n: i32): Promise<Box> { return new Box(n); }
async function makeLater(n: i32): Promise<Box> { await Context.suspend(); return new Box(n); }
function churn(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const t: i32[] = [i, i + 1]; s += t[1]; }
  return s;
}
function holdAcross(n: i32): Promise<Box> {
  const h: Promise<Box> = make(n);
  churn(50);
  Context.collect();
  return h;
}
async function user(n: i32): Promise<i32> {
  const now: Promise<Box> = make(n);
  const later: Promise<Box> = makeLater(n + 100);
  const viaSync: Promise<Box> = holdAcross(n + 200);
  for (let k: i32 = 0; k < 3; k += 1) { await Context.suspend(); churn(20); }
  Context.collect();
  const x: Box = await now;
  churn(20);
  const y: Box = await later;
  Context.collect();
  const z: Box = await viaSync;
  print(`${x.a} ${x.b} ${x.items[2]} ${y.a} ${y.b} ${y.items[1]} ${z.b} ${z.items[2]}`);
  return x.a + y.a + z.a;
}
export async function main(): Promise<void> {
  let s: i32 = 0;
  for (let i: i32 = 0; i < 4; i += 1) { s += await user(i); }
  print(`sum ${s}`);
}
