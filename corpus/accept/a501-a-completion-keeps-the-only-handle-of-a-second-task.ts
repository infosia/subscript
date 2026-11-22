// corpus: accept/a501-a-completion-keeps-the-only-handle-of-a-second-task
// purpose: A completion value that holds the only handle of a second completed task keeps that task and its completion.
// observable: The await of the second task reads the fields of its completion value, after allocations, polls, and collections.
// exercises: async-handle, completed-task, counted-field, explicit-collection, automatic-collection
// questions: compiler.md §348 rules 1 to 3, §342, §70, Q7, C8
// tsc: accepts; js-comparable: no C8 Q7: The Context coroutine and memory API has no JavaScript shim.
//
// `second` completes after one suspension, `first` after two, so the
// second task completes while only `link.next` holds it, and `link` is
// only in the frame of `first`. The local `a` of main holds the first
// task. Box(9) gives "9 box9 27".
class Box {
  a: i32;
  b: string;
  items: i32[];
  constructor(a: i32) { this.a = a; this.b = `box${a}`; this.items = [a, a * 2, a * 3]; }
}
class Link {
  next: Promise<Box>;
  tag: string;
  constructor(next: Promise<Box>, tag: string) { this.next = next; this.tag = tag; }
}
async function second(n: i32): Promise<Box> { await Context.suspend(); return new Box(n); }
async function first(n: i32): Promise<Link> {
  const link: Link = new Link(second(n), `link${n}`);
  await Context.suspend();
  await Context.suspend();
  return link;
}
function churn(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const t: i32[] = [i, i + 1]; s += t[1]; }
  return s;
}
export async function main(): Promise<void> {
  const a: Promise<Link> = first(9);
  for (let k: i32 = 0; k < 5; k += 1) { await Context.suspend(); churn(20); }
  Context.collect();
  const link: Link = await a;
  churn(20);
  Context.collect();
  const b: Box = await link.next;
  print(`chain ${link.tag} ${b.a} ${b.b} ${b.items[2]}`);
  print("done");
}
